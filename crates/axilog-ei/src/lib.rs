use std::collections::BTreeMap;
use serde_json::{json, Value};
use axilog_core::analysis::buffs::{BoonStates, BOON_IDS};
use axilog_core::analysis::damage_mods::{DamageModifierResults, DamageModifierStat};
use axilog_core::analysis::dist_outcomes::{DistOutcomes, SkillOutcomes};
use axilog_core::analysis::ei_replay::EiReplay;
use axilog_core::analysis::replay::{ActivityIntervals, Interval};
use axilog_core::analysis::healing_detail::{HealDistEntry, HealingDetail};
use axilog_core::analysis::skill_damage::SkillEntry;
use axilog_core::analysis::target_conditions::TargetConditionStates;
use axilog_core::analysis::timeseries::EnemySeries;
use axilog_core::icons::prof_icon_url;
use axilog_schema::v1::ReportV1;
use axilog_schema::Report;

mod join;

/// Test-only accessors. Not part of the supported surface.
#[doc(hidden)]
pub mod test_support {
    pub fn join(report: &axilog_schema::v1::ReportV1) -> crate::join::EiJoin<'_> {
        crate::join::EiJoin::new(report)
    }
}

// Representative real WvW team ids per color (Task 2, M2) — one id drawn
// from each of `axilog_core::wvw`'s RED/GREEN/BLUE_TEAM_IDS fixed tables
// (sourced from axibridge's `src/shared/wvwTeams.ts`, itself reconciled
// from Drevarr/EVTC_parser/gw2_data.py and
// Drevarr/GW2_EI_log_combiner/config.py). Used only as a fallback when a
// color wasn't actually observed in the log's own TEAM_CHANGE events (see
// `detected_team_ids` below) — e.g. a color with no roster presence in this
// particular fight.
fn representative_team_id(color: &str) -> u64 {
    match color {
        "red" => 697,
        "blue" => 432,
        "green" => 39,
        _ => 0,
    }
}

/// color -> the first real team id `wvw::apply` actually observed for it in
/// this log (`encounter.teams`, built from TEAM_CHANGE events). Takes the
/// team list directly (not a whole `Report`/`ReportV1`) so both the legacy
/// and the 1.0 `EncounterOut::teams` -- the latter is a straight
/// `.clone()` of the former, see `axilog_schema::v1::build_report_v1` --
/// can share the one implementation.
fn detected_team_ids(teams: &[axilog_schema::TeamOut]) -> BTreeMap<&str, u64> {
    let mut m = BTreeMap::new();
    for t in teams {
        m.entry(t.color.as_str()).or_insert(t.team_id as u64);
    }
    m
}

/// Render one interval as EI's own `[start_ms, end_ms]` two-element array
/// shape (`combatReplayData.down`/`.dead`).
fn interval_json(iv: &Interval) -> Value {
    json!([iv.start_ms, iv.end_ms])
}

/// One GW2EI-serialized floating point number (M15, Task 3).
///
/// **Every float on the combat-replay surface must go through this.** GW2EI
/// declares all of them (`combatReplayMetaData.inchToPixel`/`maps[].position`,
/// `combatReplayData.positions`/`orientations`) as C# `float`s, and .NET
/// writes a `float` as the SHORTEST decimal that round-trips through SINGLE
/// precision -- `0.009`, `246.672`, `-75.179`. `serde_json::Value` has no
/// `f32` variant: it stores every number as `f64`, and the `f64` you get by
/// widening `0.009f32` is a different real number whose own shortest
/// round-trip is `0.008999999612569809`. Feeding a widened `f32` straight
/// into `json!` therefore produces JSON TEXT that no longer matches EI's,
/// even though the two parse to values a hair apart. (This crate builds its
/// entire output through `json!`/`Value`, so there is no
/// `serde_json::to_string(&f32)` fast path available -- that one DOES print
/// `0.009`, because serde_json's string serializer has an `f32`-specific
/// `ryu` call the `Value` serializer lacks.)
///
/// Two steps, both load-bearing:
///
/// 1. **Narrow, then re-parse.** `f32::to_string` gives the shortest decimal
///    `d` that round-trips the `f32`. For magnitudes roughly in
///    `[1e-4, 1e7)` -- the only range validated here, and the only range the
///    combat-replay surface's values (map-pixel coordinates, degrees, inch-
///    to-pixel ratios) actually fall in -- this is byte-for-byte what .NET
///    writes; .NET switches to `E`-notation outside that range (e.g.
///    `1E-05`, `1E+08`) while `f32::to_string` never does, so the two
///    diverge there and this function is not validated for it. Within the
///    validated range, the nearest `f64` to `d` then prints as exactly `d`
///    again: `d` has at most 9 significant digits, while any *other* decimal
///    inside that `f64`'s ~1e-16-relative rounding interval needs ~17, so
///    `d` is also the shortest round-trip for the `f64` and `ryu` re-emits
///    it verbatim.
/// 2. **Integral values become JSON integers.** .NET (like JavaScript, which
///    is what re-serialized the reference export this crate calibrates
///    against) prints a whole-valued float as `0` / `247`, never `0.0`;
///    serde_json prints an `f64` `247.0` as `247.0`. 585 of the 297k
///    reference floats are integral, so this is not a theoretical case.
///
/// Known, non-live divergences from real .NET output -- not fixed, because
/// the reference export never exercises them (57MB of real EI output
/// contains neither token): the integral branch below emits negative zero
/// as `0`, where .NET writes `-0`; and non-finite values are emitted as
/// `null` (which is what `json!` would do anyway -- the replay engine
/// already asserts finiteness, see `ei_replay`'s
/// `assert_track_is_structurally_sound`), where .NET would write `NaN`/
/// `Infinity`/`-Infinity`.
///
/// The input is `f64` rather than `f32` because that is what
/// `axilog_core::analysis::ei_replay` stores; every value it stores is
/// itself an exactly-widened `f32` (see that module's `round_ei`), so the
/// narrowing here is lossless.
fn ei_float(v: f64) -> Value {
    let f = v as f32;
    if !f.is_finite() {
        return Value::Null;
    }
    if f.fract() == 0.0 && f.abs() < 9.0e18 {
        return Value::from(f as i64);
    }
    // `f32::to_string` -> shortest round-tripping decimal; re-parsing as
    // `f64` keeps that exact text (see step 1 above). The parse cannot fail:
    // the input is a finite decimal literal we just printed.
    Value::from(f.to_string().parse::<f64>().expect("f32 decimal re-parses as f64"))
}

/// `combatReplayData.positions`: `[[x, y], ...]` in map pixels, every
/// component through [`ei_float`].
fn ei_positions_json(positions: &[[f64; 2]]) -> Value {
    Value::Array(
        positions
            .iter()
            .map(|p| Value::Array(vec![ei_float(p[0]), ei_float(p[1])]))
            .collect(),
    )
}

/// `combatReplayData.orientations`: degrees, every value through
/// [`ei_float`].
fn ei_orientations_json(orientations: &[f64]) -> Value {
    Value::Array(orientations.iter().map(|a| ei_float(*a)).collect())
}

/// `combatReplayData.dc`: `[[start, end], ...]`, including GW2EI's
/// `long.MinValue`/`long.MaxValue` sentinel bracketing (serialized as the
/// exact `i64` bounds, which is what GW2EI's own C# writer emits -- note
/// that a reference export which has been round-tripped through JavaScript
/// will show them rounded to `-9223372036854776000`/`9223372036854776000`,
/// since they exceed `Number.MAX_SAFE_INTEGER`).
fn ei_intervals_json(intervals: &[[i64; 2]]) -> Value {
    Value::Array(intervals.iter().map(|iv| json!([iv[0], iv[1]])).collect())
}

/// One skill entry's EI-shaped JSON row (M12, Task 3) -- mirrors real EI's
/// `totalDamageDist`/`targetDamageDist`/`totalDamageTaken` entry shape
/// (verified against axibridge's `test-fixtures/boon/20260117-181030.json`,
/// `players[0].totalDamageDist[0][0]`), emitting ONLY the fields this
/// project actually computes (`axilog_schema::SkillEntryOut`'s own fields):
/// `id`, `totalDamage`, `min`, `max`, `hits`, `crit`, `flank`. Real EI's
/// sibling fields on the same entry (`totalBreakbarDamage`, `connectedHits`,
/// `glance`, `againstMoving`, `missed`, `invulned`, `interrupted`, `evaded`,
/// `blocked`, `shieldDamage`, `critDamage`, `downContribution`,
/// `indirectDamage`) aren't computed anywhere in this project's damage
/// predicate (see `axilog_core::analysis::skill_damage`'s module doc: only
/// CONTRIBUTING, `dmg > 0` events are tracked at all, with no missed/
/// blocked/evaded/etc. outcome tracking anywhere else in this schema
/// either) -- omitted rather than faked, same "don't fake absent data"
/// convention `statsTargets`/`support`/`extHealingStats` above already
/// follow. `crit`/`flank` map directly to `SkillEntryOut::crit_hits`/
/// `flank_hits` (hit COUNTS, matching real EI's own `crit`/`flank`
/// semantics exactly -- both are cleanly available, unlike the omitted
/// fields above, so they're included here even though the Task 3 brief's
/// minimal-field list only named `id`/`totalDamage`/`min`/`max`/`hits`).
fn skill_entry_ei_json(e: &axilog_schema::SkillEntryOut) -> Value {
    json!({
        "id": e.skill_id,
        "totalDamage": e.total,
        "min": e.min,
        "max": e.max,
        "hits": e.hits,
        "crit": e.crit_hits,
        "flank": e.flank_hits,
    })
}

/// One player-side distribution's rows, with MEIGAP2 row 1's outcome
/// columns merged in -- GW2EI's full `JsonDamageDist` shape for the fields
/// this project computes.
///
/// ## The union
///
/// The two inputs are keyed the same way (skill id) but do NOT have the
/// same row set. `entries` comes from
/// `axilog_core::analysis::skill_damage`, which counts CONTRIBUTING rows
/// only, so a skill whose every attempt was blocked has no entry there at
/// all; `outcomes` (`axilog_core::analysis::dist_outcomes`) counts exactly
/// those rows. GW2EI emits both kinds (its dist is keyed off the whole
/// `HealthDamageEvent` list), and the blocked-only rows are precisely what
/// axibridge's damage-mitigation table exists to read -- so this emits the
/// UNION, with the missing side's fields defaulted to zero.
///
/// ## `hits`
///
/// When outcome data is present, `hits` is GW2EI's own attempt count
/// (`dmgEvt.IsNotADamageEvent ? 0 : 1`, `JsonDamageDistBuilder.cs:52`),
/// which is what the key means in every real EI export. Without it the
/// long-standing M12 fallback stands (the CONTRIBUTING count -- see
/// [`skill_entry_ei_json`]), so a caller that asks for the distributions
/// but not their outcome columns still gets the pre-MEIGAP2 numbers rather
/// than a hole. Every first-party caller passes both together.
///
/// ## `downContribution`
///
/// Emitted per skill only when a nonzero credit exists for it, mirroring
/// GW2EI's own `int?` + `JsonIgnoreCondition.WhenWritingNull`
/// (`JsonDamageDist.cs:96-100`: absent, not `0`, when the skill contributed
/// to no down). It is `axilog_core::analysis::contribution`'s arcdps
/// methodology -- the SAME documented divergence `statsAll[0].
/// downContribution` and `statsTargets[i][0].downContribution` already
/// carry, now sliced per skill; see
/// `PlayerMetrics::downs_contribution_per_skill`. Incoming distributions
/// pass `None`, exactly as GW2EI does (`JsonActorBuilder.cs:135` hands the
/// damage-taken builder a null dictionary).
fn dist_rows_ei_json(
    entries: &[axilog_schema::SkillEntryOut],
    outcomes: Option<&[SkillOutcomes]>,
    down_contribution: Option<&BTreeMap<u32, u64>>,
) -> Value {
    let by_outcome: BTreeMap<u32, &SkillOutcomes> =
        outcomes.into_iter().flatten().map(|o| (o.skill_id, o)).collect();
    let by_entry: BTreeMap<u32, &axilog_schema::SkillEntryOut> =
        entries.iter().map(|e| (e.skill_id, e)).collect();
    let ids: std::collections::BTreeSet<u32> =
        by_entry.keys().chain(by_outcome.keys()).copied().collect();
    let rows: Vec<Value> = ids
        .into_iter()
        .map(|id| {
            let e = by_entry.get(&id);
            let o = by_outcome.get(&id);
            let mut row = serde_json::Map::new();
            row.insert("id".into(), Value::from(id));
            row.insert("totalDamage".into(), Value::from(e.map(|e| e.total).unwrap_or(0)));
            row.insert("min".into(), Value::from(e.map(|e| e.min).unwrap_or(0)));
            row.insert("max".into(), Value::from(e.map(|e| e.max).unwrap_or(0)));
            let hits = match (o, e) {
                (Some(o), _) => o.hits,
                (None, Some(e)) => e.hits,
                (None, None) => 0,
            };
            row.insert("hits".into(), Value::from(hits));
            row.insert("crit".into(), Value::from(e.map(|e| e.crit_hits).unwrap_or(0)));
            row.insert("flank".into(), Value::from(e.map(|e| e.flank_hits).unwrap_or(0)));
            if let Some(o) = o {
                row.insert("connectedHits".into(), Value::from(o.connected_hits));
                row.insert("glance".into(), Value::from(o.glance));
                row.insert("missed".into(), Value::from(o.missed));
                row.insert("evaded".into(), Value::from(o.evaded));
                row.insert("blocked".into(), Value::from(o.blocked));
                row.insert("invulned".into(), Value::from(o.invulned));
                row.insert("interrupted".into(), Value::from(o.interrupted));
                row.insert("indirectDamage".into(), Value::from(o.indirect));
            }
            if let Some(dc) = down_contribution.and_then(|m| m.get(&id)).filter(|&&d| d > 0) {
                row.insert("downContribution".into(), Value::from(*dc));
            }
            Value::Object(row)
        })
        .collect();
    Value::Array(rows)
}

/// [`skill_entry_ei_json`]'s enemy-side twin: one
/// `targets[].totalDamageDist[0][]` entry (MEIGAP Task 2c).
///
/// Same fields, ONE addition: `connectedHits`. On the player side that key
/// is deliberately omitted (see [`skill_entry_ei_json`]'s doc comment --
/// this project tracks no missed/blocked/evaded outcomes, so it cannot
/// distinguish EI's `hits` from its `connectedHits`). On the ENEMY side the
/// distinction is forced, because axibridge's damage-mitigation math
/// divides by `connectedHits` specifically
/// (`computePlayerAggregation.ts:277-286`, `avg = totalDamage /
/// connectedHits`) and would otherwise get a `0` denominator and drop the
/// skill entirely (`if (!enemy.hasSkill || enemy.hits <= 0) return;`).
///
/// So the CONTRIBUTING-hit count this project tracks is emitted under the
/// key whose EI meaning it actually reproduces -- `connectedHits` is
/// `dmgEvt.HasHit ? 1 : 0` (`JsonDamageDistBuilder.cs:72`), and a
/// blocked/evaded/missed/invulned row (the `HasHit == false` cases) carries
/// `dmg == 0` and is skipped by this project's damage predicate. `hits`
/// (EI's attempt count) stays absent rather than being filled with a number
/// that means something else; the exact residual is stated in
/// `skill_damage::build_enemy_dist`'s doc comment.
/// `extHealingStats.totalHealingDist[0]` / `extBarrierStats
/// .totalBarrierDist[0]` (MEIGAP Task 3a) -- GW2EI's `EXTJsonHealingDist`
/// / `EXTJsonBarrierDist` shape, field-for-field.
///
/// `total_key` is `"totalHealing"` or `"totalBarrier"`; `indirect_key`
/// follows it (`"indirectHealing"` / `"indirectBarrier"`).
/// `with_downed` adds `totalDownedHealing`, which exists only on the
/// healing side (`EXTJsonBarrierDist` has no downed field at all).
fn heal_dist_json(rows: &[HealDistEntry], total_key: &str, with_downed: bool) -> Value {
    Value::Array(
        rows.iter()
            .map(|r| {
                let mut o = serde_json::Map::new();
                o.insert("id".into(), Value::from(r.skill_id));
                o.insert(total_key.into(), Value::from(r.total));
                if with_downed {
                    o.insert("totalDownedHealing".into(), Value::from(r.total_downed));
                }
                o.insert("hits".into(), Value::from(r.hits));
                o.insert("min".into(), Value::from(r.min));
                o.insert("max".into(), Value::from(r.max));
                o.insert(
                    if with_downed { "indirectHealing".into() } else { "indirectBarrier".to_string() },
                    Value::from(r.indirect),
                );
                Value::Object(o)
            })
            .collect(),
    )
}

fn enemy_skill_entry_ei_json(e: &SkillEntry) -> Value {
    json!({
        "id": e.skill_id,
        "totalDamage": e.total,
        "min": e.min,
        "max": e.max,
        "connectedHits": e.hits,
        "crit": e.crit_hits,
        "flank": e.flank_hits,
    })
}

/// `damageModifiers[].damageModifiers[].damageGain` (M16, Task 3).
///
/// **Deliberately NOT [`ei_float`].** That helper narrows through `f32`
/// because the replay coordinates it was written for are C# `float`s
/// (M15's lesson). `DamageGain` is a C# **`double`** on both sides of the
/// pipeline -- `DamageModifierStat.DamageGain`
/// (`GW2EIEvtcParser/EIData/Statistics/DamageModifierStat.cs:7`) and
/// `JsonDamageModifierItem.DamageGain`
/// (`GW2EIJSON/.../JsonDamageModifierData.cs:28`) are both `double`, and it
/// is `Math.Round(x, 3)`-ed at construction (`:14`,
/// `ParserHelper.DamageModGainDigit = 3`). Narrowing it to `f32` would
/// corrupt exactly the values that matter: `279362` is past `f32`'s
/// 24-bit integer range, and `-9690.778` has no `f32` representation whose
/// shortest decimal is `-9690.778`.
///
/// So the value is emitted as the `f64` it already is, with one adjustment:
/// a whole number is emitted as an INTEGER, because that is what .NET's
/// serializer writes (the reference export has `"damageGain": 10592`, never
/// `10592.0`) while `serde_json` would write `10592.0`. Everything else
/// goes out as `f64`, whose shortest round-tripping decimal is the same
/// text .NET produces for a `Math.Round(_, 3)`-ed double -- verified
/// against all 13,905 `damageGain` values in the reference export (5,781
/// integral, 8,124 with 1-3 decimals, none with more).
/// A `double` field whose whole-number values .NET writes WITHOUT a
/// decimal point (`"breakbarDamage": 0`, `[0, 100]`), the same
/// serializer behaviour [`ei_damage_gain`] documents at length -- factored
/// out here because MEIGAP2 added two more `double` surfaces with the same
/// need (`dpsAll[0].breakbarDamage` and `healthPercents`' percent column).
///
/// Deliberately NOT [`ei_float`]: neither of those is a C# `float`.
fn ei_double(v: f64) -> Value {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9.0e15 {
        return Value::from(v as i64);
    }
    Value::from(v)
}

/// Raw arcdps breakbar units -> GW2EI's own number: `BreakbarDamageEvent`'s
/// ctor is `BreakbarDamage = Math.Round(evtcItem.Value / 10.0, 1)`
/// (`ParsedData/CombatEvents/NonDamageEvents/BreakbarDamageEvent.cs:8`) and
/// `DamageStatistics.cs:60` rounds the SUM to 1 decimal again. Dividing an
/// integer sum by 10 already lands exactly on 1 decimal, so the second
/// rounding is a no-op and is not re-applied (doing so in binary floating
/// point could only introduce error, never remove any).
fn ei_breakbar(raw_value_sum: u64) -> Value {
    ei_double(raw_value_sum as f64 / 10.0)
}

fn ei_damage_gain(v: f64) -> Value {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9.0e15 {
        return Value::from(v as i64);
    }
    Value::from(v)
}

/// `Math.Round(x, 3)` with .NET's default `MidpointRounding.ToEven`.
///
/// Hand-rolled rather than `f64::round_ties_even`, which is only stable
/// since Rust 1.77 and this workspace's MSRV is 1.74 (`Cargo.toml`'s
/// `rust-version`). Ties land on the EVEN scaled integer, exactly as .NET
/// does; every other value rounds normally. `floor` is toward negative
/// infinity, so the parity test is correct for negative inputs too (none
/// of this crate's callers produce them, but the helper is general).
fn round3_ties_even(x: f64) -> f64 {
    let scaled = x * 1000.0;
    let floor = scaled.floor();
    let frac = scaled - floor;
    let rounded = if frac == 0.5 {
        // The tie: land on the even scaled integer.
        if (floor as i64) % 2 == 0 { floor } else { floor + 1.0 }
    } else if frac > 0.5 {
        floor + 1.0
    } else {
        floor
    };
    rounded / 1000.0
}

/// One GW2EI buff-percentage number, `Math.Round(x, ParserHelper.BuffDigit)`
/// with `BuffDigit = 3` (MEIGAP Task 1a).
///
/// Every value GW2EI writes into `selfBuffs`/`groupBuffs`/`squadBuffs`'
/// `buffData[]` goes through that rounding before serialization
/// (`GW2EIEvtcParser/EIData/Statistics/BuffStatistics.cs:116-121` for the
/// duration branch, `:135-141` for the intensity branch, `:190-195`/
/// `:211-216` for the `GetBuffsForSelf` twin; `ParserHelper.BuffDigit = 3`
/// at `GW2EIEvtcParser/ParserHelpers/ParserHelper.cs:24`), so the reference
/// export never carries more than three decimals on any of them -- verified
/// over all 6,371 `generation` values in `fixtures/local/
/// wvw-postrework.ei.json`.
///
/// .NET's `Math.Round(double, int)` is half-to-EVEN (`MidpointRounding.
/// ToEven` is the documented default), which is what [`round3_ties_even`]
/// below reproduces. Whole values are emitted as JSON
/// integers, matching .NET's own serializer (`"generation": 0`, never
/// `0.0`) -- the same adjustment [`ei_damage_gain`] already makes for
/// `damageGain`, and for the same reason (`serde_json` would otherwise
/// write `0.0`).
///
/// **Deliberately NOT [`ei_float`]**: these are C# `double`s
/// (`JsonBuffsGenerationData.Generation`, `GW2EIJSON/JsonActorUtilities/
/// JsonPlayerUtilities/JsonPlayerBuffsGeneration.cs:16-45`), not the
/// `float`s M15's replay surface narrows through.
fn ei_buff_pct(v: f64) -> Value {
    if !v.is_finite() {
        return Value::Null;
    }
    let r = round3_ties_even(v);
    if r.fract() == 0.0 && r.abs() < 9.0e15 {
        return Value::from(r as i64);
    }
    Value::from(r)
}

/// One GW2EI duration-in-seconds number: `Math.Round(ms / 1000.0,
/// ParserHelper.TimeDigit)` with `TimeDigit = 3` (MEIGAP Task 1c).
///
/// The convention every `*Time` field on `JsonStatistics` uses -- e.g.
/// `GW2EIEvtcParser/EIData/Statistics/DefensePerTargetStatistics.cs:69`
/// (`boonStripsTime`/`conditionCleansesTime`) and
/// `SupportStatistics.cs:78` (the outgoing twin). Half-to-even and
/// whole-value-as-integer for the same reasons [`ei_buff_pct`] documents.
fn ei_time_secs(ms: u64) -> Value {
    let r = round3_ties_even(ms as f64 / 1000.0);
    if r.fract() == 0.0 {
        return Value::from(r as i64);
    }
    Value::from(r)
}

/// `buffUptimes[].states`/`.statesPerSource`: `[[time, stacks], ...]`
/// (MEIGAP Task 1b). A `buffUptimes` entry for a boon this player never
/// held gets `[]`, matching GW2EI's own empty-graph case
/// (`JsonBuffsUptimeBuilder.cs:68-76` returns an empty list there).
fn ei_states_json(states: &[(u64, u32)]) -> Value {
    Value::Array(states.iter().map(|&(t, v)| json!([t, v])).collect())
}

/// One of the three boon-generation attribution arrays
/// (`selfBuffs`/`groupBuffs`/`squadBuffs`), `pick` selecting which scope of
/// [`axilog_schema::GenerationOut`] to read -- see the call sites' doc
/// comment for the GW2EI citation trail.
///
/// `keep_zero` is EI's own id-set rule, which differs between the arrays --
/// see the call site for the citation: `selfBuffs` keeps every id
/// `buffUptimes` carries (zeros included), `groupBuffs`/`squadBuffs` are
/// filtered to buffs this player is a recorded source for
/// (`BuffStatistics.cs:66,100`'s `hasGeneration`).
///
/// **What "a recorded source for" means, corrected by the MSMALL review.**
/// `hasGeneration` is `buffDistribution.HasSrc(boon.ID, srcAgentItem)`, and
/// `HasSrc` is a bare key-presence test on the per-buff
/// `Dictionary<AgentItem, BuffDistributionItem>`
/// (`BuffDistribution.cs:78-81`) -- NOT a test that the source generated
/// any held time. `SimulationItem.AddWaste` registers a source with
/// `new BuffDistributionItem(0, 0, value, 0, 0, 0)`, i.e. `Value == 0` and
/// `Waste == value` (`SimulationItem.cs:99-116`). So a WASTE-ONLY source --
/// one whose every stack was overwritten or stripped before it held any
/// time -- is a recorded source, and EI emits its row.
///
/// This filter therefore keeps a row when EITHER the generation or the
/// wasted value is non-zero. Filtering on generation alone (what it did
/// before) silently dropped real EI rows: measured on
/// `fixtures/wvw-small.anon.zevtc`, 9 `groupBuffs`/`squadBuffs` cells with
/// `generation == 0` but substantial waste, including an Aegis
/// `groupBuffs` entry at `wasted = 18.247`.
///
/// The two channels this project does NOT model (overstack, extension) can
/// also register a source in EI. A source visible to EI through ONLY those
/// is still missed here -- unavoidable while those channels are unmodelled,
/// and unchanged by this fix.
fn buff_generation_json(
    boons: &[axilog_schema::BoonOut],
    pick: fn(&axilog_schema::GenerationOut) -> f64,
    pick_wasted: fn(&axilog_schema::GenerationOut) -> f64,
    keep_zero: bool,
) -> Value {
    Value::Array(
        boons
            .iter()
            .filter(|b| {
                keep_zero || pick(&b.generation) > 0.0 || pick_wasted(&b.generation) > 0.0
            })
            .map(|b| {
                json!({
                    "id": b.id,
                    "buffData": [ {
                        "generation": ei_buff_pct(pick(&b.generation)),
                        // MSMALL item 2: `BuffStatistics.Wasted`, the same
                        // rounding/scale as `generation` (verified: the
                        // duration/intensity branches at
                        // `BuffStatistics.cs:116-141` and `:190-216` treat
                        // the two identically). See
                        // `axilog_core::analysis::buffs::generation::
                        // WasteRecord` for the three GW2EI sites that
                        // produce it.
                        "wasted": ei_buff_pct(pick_wasted(&b.generation)),
                    } ],
                })
            })
            .collect(),
    )
}

/// One `{ hitCount, totalHitCount, damageGain, totalDamage }` item, wrapped
/// in EI's per-PHASE array (`JsonDamageModifierData.DamageModifiers`,
/// "Length == # of phases"). This project does not model phases, so it is
/// always a single element -- the same "one array standing in for phase 0
/// == the whole fight" convention `statsAll`/`totalDamageDist` already use.
fn ei_damage_mod_rows(rows: &[(i32, &DamageModifierStat)]) -> Vec<Value> {
    rows.iter()
        .map(|(id, s)| {
            json!({
                "id": id,
                "damageModifiers": [ {
                    "hitCount": s.hit_count,
                    "totalHitCount": s.total_hit_count,
                    "damageGain": ei_damage_gain(s.damage_gain),
                    "totalDamage": s.total_damage,
                } ],
            })
        })
        .collect()
}

/// Splits one actor's `(id, stat)` rows into EI's outgoing/incoming pair.
/// The sign of the id IS the direction (`DamageModifier.cs:26`), so no
/// definition lookup is needed.
fn ei_damage_mod_split(rows: Vec<(i32, &DamageModifierStat)>) -> (Vec<Value>, Vec<Value>) {
    let (incoming, outgoing): (Vec<_>, Vec<_>) = rows.into_iter().partition(|(id, _)| *id < 0);
    (ei_damage_mod_rows(&outgoing), ei_damage_mod_rows(&incoming))
}

/// The side-channel inputs [`to_ei_json`] needs on top of the native
/// [`Report`] (M16 Task 1).
///
/// Everything here is EI-SHAPE data that the native schema deliberately does
/// not carry (see each field), computed by the caller and handed in
/// alongside the report. It is an options struct rather than a positional
/// argument list because this surface only grows: M11 added `activity`, M15
/// added `replay`, and further EI-only inputs are expected. Adding a field
/// is then a source-compatible change for in-workspace callers that build it
/// with `..Default::default()`, and a one-line change for the rest.
///
/// [`Default`] is the "nothing available" case (`activity: &[]`,
/// `replay: None`), which every field's own default behaviour treats as a
/// harmless zero/empty, never a panic — so
/// `to_ei_json(&report_v1, &report, &EiInputs::default())` is always valid.
///
/// It is `Copy` and holds only borrows, so passing it costs nothing and it
/// never takes ownership of the caller's buffers.
#[derive(Debug, Clone, Copy, Default)]
pub struct EiInputs<'a> {
    /// `activity` (M11 Task 3): per-player down/dead intervals + first/last-
    /// aware bounds from
    /// `axilog_core::analysis::replay::build_activity_intervals`
    /// — ALWAYS computed by every caller (CLI/Node/Python), unlike
    /// `--replay`'s position track, since intervals are cheap (see that
    /// function's module doc). Positionally joined to `report.players` (both
    /// built by iterating `enc.players` in the same order — see
    /// `build_activity_intervals`'s doc comment); leave it empty if
    /// unavailable (every field this powers is then a harmless zero/empty
    /// default, not a panic).
    pub activity: &'a [ActivityIntervals],
    /// `replay` (M15 Task 3): the GW2EI-shape fixed-rate combat replay from
    /// `axilog_core::analysis::ei_replay::build_ei_replay_auto`, or `None`.
    /// This is the OPT-IN gate for `combatReplayData.{positions,
    /// orientations, dc, iconURL}` and the top-level
    /// `combatReplayMetaData`: every caller (CLI/Node/Python) computes it
    /// exactly when `--replay`/SDK `replay: true` was requested — i.e. the
    /// same request that populates `axilog_schema::Report::replay` — so the
    /// presence of this input is the "was replay requested" signal,
    /// mirroring how the `skill_damage`/`timeseries`/`rotation` blocks key
    /// off `PlayerOut`'s own `Option` presence rather than a separate flag.
    ///
    /// It arrives as a side-channel input rather than a `Report` field for
    /// the same reason `activity` does: it is EI-shape data (map PIXELS on
    /// GW2EI's own 300ms grid, GW2EI's sentinel-bracketed `dc`), which the
    /// native schema deliberately does not carry — `Report::replay` is this
    /// project's own narrower world-unit shape, computed by a different
    /// engine (`axilog_core::analysis::replay`), and widening it would
    /// change the native `--replay` JSON.
    ///
    /// Positionally joined to `report.players` (GW2EI-shape tracks are built
    /// by iterating `enc.players` then the `is_player` entries of
    /// `enc.enemies`, exactly the orders `report.players`/
    /// `report.ei_targets` use), and ignored entirely if that length
    /// invariant does not hold.
    ///
    /// **Size (measured, `fixtures/wvw-small.anon.zevtc`: 41 players, 32
    /// enemy-player targets, 49s):** `axilog parse --format ei-json` grows
    /// 544,372 -> 1,548,945 bytes pretty-printed (+184%), 216,173 -> 524,056
    /// bytes compact (+142%), for 6,894 player + 4,662 enemy position
    /// samples (and as many orientations). It scales with `players x
    /// fight_seconds / 0.3`, so a 6-minute 50-player fight is an order of
    /// magnitude bigger — which is why it stays opt-in.
    pub replay: Option<&'a EiReplay>,
    /// `modifiers` (M16 Task 3): the damage-modifier engine's output from
    /// `axilog_core::analysis::damage_mods::evaluate_catalog_full`, or
    /// `None`. This is the OPT-IN gate for the four per-player arrays
    /// (`damageModifiers`, `incomingDamageModifiers`,
    /// `damageModifiersTarget`, `incomingDamageModifiersTarget`) and the
    /// top-level `damageModMap`; every caller computes it exactly when
    /// `--modifiers`/SDK `modifiers: true` was requested, i.e. the same
    /// request that populates `axilog_schema::PlayerOut::damage_mods`.
    ///
    /// It arrives here as the RAW engine output rather than being read back
    /// off `PlayerOut::damage_mods` because the native block is deliberately
    /// whole-fight only: `DamageModifierResults::per_target` has no native
    /// counterpart (measured on the committed fixture it is 854,077 bytes
    /// against the whole-fight arrays' 76,611 -- an 11x multiplier, and
    /// the same ratio the reference export shows: 1.34 MB vs 108 KB), and
    /// EI needs it in a positional `[targetIndex]` shape keyed to
    /// `targets[]`. Rendering both surfaces from the one core result keeps
    /// them from drifting.
    ///
    /// Joined to `report.players` by `PlayerOut::agent_addr` and to
    /// `report.ei_targets` by `EnemyOut::id` -- both real keys, not
    /// positions, so a mismatch yields empty arrays rather than
    /// mis-attributed rows.
    ///
    /// **Size (measured, `fixtures/wvw-small.anon.zevtc`, compact):**
    /// `--format ei-json` grows 216,173 -> 1,170,570 bytes (+441.5%) --
    /// `damageModifiers` 32,121, `incomingDamageModifiers` 44,490,
    /// `damageModifiersTarget` 497,702, `incomingDamageModifiersTarget`
    /// 356,375, `damageModMap` 19,325.
    pub modifiers: Option<&'a DamageModifierResults>,
    /// `boon_states` (MEIGAP Task 1b): the GW2EI-shape boon stack timelines
    /// from `axilog_core::analysis::buffs::states::build`, or `None`. This
    /// is the OPT-IN gate for `buffUptimes[].states` and
    /// `buffUptimes[].statesPerSource`.
    ///
    /// GW2EI puts those same two arrays behind its own
    /// `RawFormatTimelineArrays` setting
    /// (`GW2EIBuilders/JsonModels/JsonActorUtilities/JsonBuffsUptimeBuilder.cs:52`)
    /// -- the setting axibridge already maps onto axilog's `--timeseries`
    /// -- so every caller computes this exactly when `--timeseries`/SDK
    /// `timeseries: true` was requested, i.e. the same request that
    /// populates `axilog_schema::PlayerOut::per_second`. Reproducing EI's
    /// own gate rather than inventing one is what keeps the two payloads
    /// shaped alike under the same settings.
    ///
    /// It arrives as a side-channel input rather than a `Report` field for
    /// the same reason `activity`/`replay` do: it is EI-SHAPE data
    /// (LOG-RELATIVE ms with GW2EI's mandatory leading `[0, 0]` pair,
    /// keyed by source CHARACTER NAME), which the native schema
    /// deliberately does not carry -- `Metrics::boons` is this project's
    /// own absolute-time, addr-keyed shape.
    ///
    /// Joined to `report.players` by `PlayerOut::agent_addr` -- a real key,
    /// not a position, so a mismatch yields an absent entry rather than a
    /// mis-attributed timeline.
    ///
    /// **Size (measured, `fixtures/wvw-small.anon.zevtc`, pretty-printed,
    /// 41 players x 12 boons over a 49s fight):** `--format ei-json
    /// --timeseries` grows 3,878,210 -> 4,855,657 bytes (**+25.2%**). The
    /// flagless payload is byte-identical to before (763,450 both sides).
    /// It scales with transitions, i.e. roughly `players x boons x
    /// applications`, which is why it stays behind the flag GW2EI itself
    /// puts it behind.
    pub boon_states: Option<&'a BoonStates>,
    /// `enemy_series` (MEIGAP Task 2b): per-enemy cumulative outgoing-damage
    /// series from `axilog_core::analysis::timeseries::build_enemy_series`,
    /// or `None`. The OPT-IN gate for `targets[].damage1S` and
    /// `targets[].powerDamage1S`.
    ///
    /// GW2EI gates the same two arrays on `RawFormatTimelineArrays`
    /// (`GW2EIBuilders/JsonModels/JsonActors/JsonActorBuilder.cs:63-80`,
    /// the shared actor builder `JsonNPCBuilder` runs first), so every
    /// caller computes this exactly when `--timeseries`/SDK
    /// `timeseries: true` was requested -- the same request that populates
    /// `PlayerOut::per_second`.
    ///
    /// Side-channel rather than a `Report` field because it is keyed by
    /// enemy, and the native schema carries no per-enemy block at all
    /// (`Report::ei_targets` is identity-only, and is itself
    /// `#[serde(skip)]`).
    ///
    /// **Size (measured, `fixtures/wvw-small.anon.zevtc`, pretty-printed):**
    /// see this crate's `targets[]` block comment.
    pub enemy_series: Option<&'a BTreeMap<u64, EnemySeries>>,
    /// `enemy_dist` (MEIGAP Task 2c): per-enemy outgoing per-skill damage
    /// distribution from `axilog_core::analysis::skill_damage::
    /// build_enemy_dist`, or `None`. The OPT-IN gate for
    /// `targets[].totalDamageDist`.
    ///
    /// Unlike its two siblings here, GW2EI emits this UNCONDITIONALLY
    /// (`JsonActorBuilder.cs:87` sits outside every
    /// `RawFormatTimelineArrays` block). It rides `--skill-damage` here for
    /// the same reason the player-side `totalDamageDist` already does --
    /// payload -- and axibridge hardcodes that flag to `true`, so the read
    /// surface is unchanged (see the cutover report's flag table).
    pub enemy_dist: Option<&'a BTreeMap<u64, Vec<SkillEntry>>>,
    /// `target_conditions` (MEIGAP Task 2d): per-(enemy, condition)
    /// source-split stack timelines from
    /// `axilog_core::analysis::target_conditions::build`, or `None`. The
    /// OPT-IN gate for `targets[].buffs[].id`/`.statesPerSource` -- and for
    /// the fourteen CONDITION entries this adapter then adds to `buffMap`,
    /// without which axibridge's `resolveBuffMetaById` returns nothing and
    /// the whole array is skipped (`conditionsMetrics.ts:311-314`).
    ///
    /// Gated on `--timeseries`: `statesPerSource` sits inside GW2EI's own
    /// `RawFormatTimelineArrays` block
    /// (`JsonBuffsUptimeBuilder.cs:52`), the same gate the player-side
    /// [`Self::boon_states`] rides.
    pub target_conditions: Option<&'a TargetConditionStates>,
    /// `healing_detail` (MEIGAP Task 3a): per-ally / per-skill / per-second
    /// healing and barrier from
    /// `axilog_core::analysis::healing_detail::build`, or `None` (which is
    /// also what that function returns on a log with no healing
    /// extension). Positionally joined to `report.players`, in both the
    /// outer index and the inner ally index.
    ///
    /// The OPT-IN gate for `extHealingStats.outgoingHealingAllies` /
    /// `.totalHealingDist` / `.healing1S` and
    /// `extBarrierStats.outgoingBarrierAllies` / `.totalBarrierDist`. The
    /// three families are gated SEPARATELY once this input is present --
    /// see [`Self::healing_series`] and [`Self::healing_dist`], which are
    /// the two flag bits the caller sets alongside it.
    ///
    /// Side-channel rather than a `Report` field for the same reason
    /// [`Self::enemy_dist`] is: it is a per-(player, ally) matrix and a
    /// per-skill map that the native schema deliberately reduces to the
    /// five `HealingOut` scalars.
    pub healing_detail: Option<&'a HealingDetail>,
    /// Emit `extHealingStats.healing1S` -- set by the caller exactly when
    /// `--timeseries`/SDK `timeseries: true` was requested, GW2EI's own
    /// gate on that field
    /// (`EXTJsonPlayerHealingStatsBuilder.cs:30`, inside
    /// `if (settings.RawFormatTimelineArrays)`). Ignored when
    /// [`Self::healing_detail`] is `None`.
    pub healing_series: bool,
    /// Emit `extHealingStats.outgoingHealingAllies` / `.totalHealingDist`
    /// and `extBarrierStats.outgoingBarrierAllies` / `.totalBarrierDist` --
    /// set by the caller exactly when `--skill-damage`/SDK
    /// `skill_damage: true` was requested. GW2EI emits all four
    /// unconditionally; they ride the per-skill flag here for PAYLOAD (the
    /// ally matrices alone are +36.0% on the flagless committed fixture and
    /// grow quadratically in squad size), the same treatment
    /// `totalDamageDist` already gets, and axibridge hardcodes that flag to
    /// `true`. Ignored when [`Self::healing_detail`] is `None`.
    pub healing_dist: bool,
    /// `dist_outcomes` (MEIGAP2 row 1): per-skill hit-OUTCOME columns for
    /// the two player-side distributions, from
    /// `axilog_core::analysis::dist_outcomes::build`, or `None`. Keyed by
    /// player representative addr (`PlayerOut::agent_addr`).
    ///
    /// The OPT-IN gate for `totalDamageDist[][]`/`totalDamageTaken[][]`'s
    /// `connectedHits`/`glance`/`missed`/`evaded`/`blocked`/`invulned`/
    /// `interrupted`/`indirectDamage` columns -- set by the caller on
    /// `--skill-damage`, the flag that already gates the distributions
    /// themselves, so this can never annotate rows that are not emitted.
    pub dist_outcomes: Option<&'a BTreeMap<u64, DistOutcomes>>,
}

/// A lazily-serialized JSON array (MSTREAM).
///
/// `len` elements, each produced on demand by `f(index)` and dropped as soon
/// as the serializer has consumed it. This is the whole point of the
/// streaming path: `players`/`targets` are ~99% of the ei-json document, and
/// materializing them as a `Vec<Value>` (which is what `.collect()` did
/// before MSTREAM) is what made peak RSS scale with the WHOLE document
/// rather than with one row of it.
///
/// The element builder is `FnMut` (the `targets` one walks a replay-track
/// iterator across calls) so it lives behind a `RefCell`: `Serialize::
/// serialize` only gets `&self`. Serializing the same `LazySeq` twice would
/// therefore resume a partly-consumed builder — every [`EiDoc`] is built
/// fresh per call and serialized exactly once, which is what both entry
/// points below do.
struct LazySeq<'f, 'a> {
    len: usize,
    rows: &'f LazyRows<'a>,
}

/// A stateful row builder plus its take-once guard (MSTREAM review).
///
/// The guard has to live HERE, next to the `FnMut`, rather than on
/// [`LazySeq`]: `LazySeq` values are constructed fresh inside
/// [`EiDoc::serialize`], so a flag on them would reset on every
/// serialization and never catch the bug it exists to catch. The closure
/// (owned by [`EiDoc`]) is the actual once-consumable resource.
struct LazyRows<'a> {
    f: std::cell::RefCell<Box<dyn FnMut(usize) -> Value + 'a>>,
    /// Set on first consumption; a second one would silently resume the
    /// partly-consumed builder and emit garbage rather than the same array
    /// again. Debug-only -- release builds keep the (correct,
    /// single-serialization) path with no added behaviour.
    consumed: std::cell::Cell<bool>,
}

impl<'a> LazyRows<'a> {
    fn new(f: Box<dyn FnMut(usize) -> Value + 'a>) -> Self {
        LazyRows { f: std::cell::RefCell::new(f), consumed: std::cell::Cell::new(false) }
    }
}

impl serde::Serialize for LazySeq<'_, '_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        debug_assert!(
            !self.rows.consumed.replace(true),
            "LazySeq is take-once: its FnMut element builder is stateful, so a \
             second serialization would resume a partly-consumed builder \
             instead of re-emitting the array (see LazyRows' doc comment)"
        );
        let mut f = self.rows.f.borrow_mut();
        let mut seq = s.serialize_seq(Some(self.len))?;
        for i in 0..self.len {
            // `f(i)` is built, handed to the serializer, and dropped before
            // the next iteration allocates anything.
            seq.serialize_element(&f(i))?;
        }
        seq.end()
    }
}

/// The whole ei-json document, as a *plan* rather than a materialized tree
/// (MSTREAM).
///
/// Every small section (`buffMap`, `skillMap`, `damageModMap`,
/// `combatReplayMetaData`, `wvWMapData`) is an ordinary eagerly-built
/// `Value` — together they are kilobytes. The two big ones (`players`,
/// `targets`) are closures invoked one row at a time by [`LazySeq`].
///
/// [`Self::serialize`] emits the root keys in the exact order a
/// `BTreeMap<String, _>` would — i.e. byte-wise ascending, which is the
/// alphabetized-key convention every consumer of this format relies on and
/// what the pre-MSTREAM `json!`-into-`serde_json::Map` root produced for
/// free. That order is not merely convention here: it is load-bearing for
/// byte-identity, and it is regression-tested by
/// `streaming_matches_value_tree_byte_for_byte`, which diffs the streamed
/// text against `to_ei_json`'s tree (the tree re-sorts through `BTreeMap`,
/// so any hand-ordering mistake below shows up as a diff).
/// EI prefixes catalog keys by kind (`b1187`, `s5491`, `d64`); native
/// stores bare ids. One helper so the three maps cannot disagree.
///
/// The id is `i64` rather than `u32` because `damageModMap`'s ids are
/// SIGNED -- `d-128` is a real key, and the sign is what distinguishes an
/// incoming modifier from an outgoing one.
fn ei_catalog_key(prefix: char, id: i64) -> String {
    format!("{prefix}{id}")
}

struct EiDoc<'a> {
    fight_name: String,
    duration_ms: u64,
    recorded_by: Option<&'a str>,
    player_count: usize,
    player_json: LazyRows<'a>,
    target_count: usize,
    target_json: LazyRows<'a>,
    buff_map: BTreeMap<String, Value>,
    skill_map: BTreeMap<String, Value>,
    damage_mod_map: Option<BTreeMap<String, Value>>,
    combat_replay_meta: Option<Value>,
    wvw_map_data: Value,
}

impl serde::Serialize for EiDoc<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        // ---- byte-wise ascending key order; do not reshuffle ----
        m.serialize_entry("buffMap", &self.buff_map)?;
        if let Some(meta) = &self.combat_replay_meta {
            m.serialize_entry("combatReplayMetaData", meta)?;
        }
        if let Some(dm) = &self.damage_mod_map {
            m.serialize_entry("damageModMap", dm)?;
        }
        m.serialize_entry("durationMS", &self.duration_ms)?;
        m.serialize_entry("eliteInsightsVersion", &Value::Null)?;
        m.serialize_entry("fightName", &self.fight_name)?;
        m.serialize_entry(
            "players",
            &LazySeq { len: self.player_count, rows: &self.player_json },
        )?;
        m.serialize_entry("recordedBy", &self.recorded_by)?;
        m.serialize_entry("skillMap", &self.skill_map)?;
        m.serialize_entry("success", &true)?;
        m.serialize_entry(
            "targets",
            &LazySeq { len: self.target_count, rows: &self.target_json },
        )?;
        m.serialize_entry("wvWMapData", &self.wvw_map_data)?;
        m.end()
    }
}

/// Render a [`Report`] plus its EI-only side inputs as Elite-Insights-
/// compatible JSON, as a *streaming* document (MSTREAM).
///
/// This is the single source of truth for the ei-json surface. Both public
/// entry points go through it:
///
/// * [`write_ei_json`] serializes it straight to an `io::Write` — nothing
///   bigger than one player row is ever resident.
/// * [`to_ei_json`] serializes it into a `serde_json::Value` for the SDKs,
///   which need a tree regardless (napi/pythonize walk one).
///
/// See [`EiInputs`] for what each input gates; `EiInputs::default()` renders
/// everything that is derivable from the `Report` alone.
fn ei_doc<'a>(report: &'a ReportV1, legacy: &'a Report, inputs: &EiInputs<'a>) -> EiDoc<'a> {
    let EiInputs {
        activity,
        replay,
        modifiers,
        boon_states,
        enemy_series,
        enemy_dist,
        target_conditions,
        healing_detail,
        healing_series,
        healing_dist,
        dist_outcomes,
    } = *inputs;
    // Positional-join guards, same convention `replay` uses just below: a
    // hand-built `Report` (every unit test) can violate them, and dropping
    // the whole surface beats mis-attributing one player's healing to
    // another.
    let healing_detail = healing_detail.filter(|d| d.len() == legacy.players.len());
    // Positional join guard: the tracks must be `legacy.players` followed by
    // the enemy-PLAYER subset of `legacy.ei_targets`, in those orders. A
    // caller that hand-builds a `Report` (every unit test below) can violate
    // it; rather than mis-attribute one player's movement to another, drop
    // the whole replay surface.
    let enemy_player_count = legacy.ei_targets.iter().filter(|e| e.is_player).count();
    let replay = replay.filter(|r| r.tracks.len() == legacy.players.len() + enemy_player_count);
    // `detected` feeds the player-row and target-row closures below (each
    // clones it and builds its own locally-shadowed `team_id_for` -- see
    // `detected_players`/`detected_targets`); `wvWMapData`'s own team-id
    // lookup is a separate computation off `report` now (Task 3), not this
    // one, so there is no single shared `team_id_for` left at this scope.
    let detected = detected_team_ids(&legacy.encounter.teams);

    // M10 Task 1: whole-fight seconds for the healing-extension `hps`/`bps`
    // fields below -- same `(duration_ms / 1000.0).max(1.0)` convention
    // `axilog_core::analysis::analyze` itself uses for `dps` (avoids a
    // divide-by-zero on a degenerate zero-duration log).
    let duration_secs = (legacy.encounter.duration_ms as f64 / 1000.0).max(1.0);

    // MSTREAM: one player row, built on demand. Previously the body of a
    // `report.players.iter().enumerate().map(..).collect::<Vec<Value>>()`;
    // the body itself is unchanged, only its binding form. `detected` is
    // cloned in (a <=3-entry `BTreeMap<&str, u64>`) so the outer
    // `team_id_for` and the `targets` builder can each keep their own.
    let detected_players = detected.clone();
    // Task 4: `players[]` is indexed by EI position, and `source_order`
    // is what maps that position back to a native entity id. Resolved
    // once per row here so every block lookup below shares one join --
    // the whole reason `EiJoin` exists (see `join.rs`'s module doc).
    let player_ids: Vec<u32> = report.source_order.players().to_vec();
    let player_json: Box<dyn FnMut(usize) -> Value + 'a> = Box::new(move |player_idx: usize| {
        let p = &legacy.players[player_idx];
        // The native rows backing this player. Blocks are `Option` (the
        // compute gates) and `by_entity` is sparse, so each is an
        // `Option<&Row>` and every read below carries its own zero
        // default -- an absent row means "this entity did nothing of that
        // kind", which is genuinely zero, not unknown.
        let entity_id = player_ids.get(player_idx).copied();
        let n_damage = entity_id
            .and_then(|id| report.blocks.damage.as_ref()?.by_entity.get(id));
        let n_hit_stats = entity_id
            .and_then(|id| report.blocks.hit_stats.as_ref()?.by_entity.get(id));
        let n_defenses = entity_id
            .and_then(|id| report.blocks.defenses.as_ref()?.by_entity.get(id));
        let n_cc = entity_id.and_then(|id| report.blocks.cc.as_ref()?.by_entity.get(id));
        let n_support = entity_id
            .and_then(|id| report.blocks.support.as_ref()?.by_entity.get(id));
        let n_contribution = entity_id
            .and_then(|id| report.blocks.contribution.as_ref()?.by_entity.get(id));
        let n_rotation = entity_id
            .and_then(|id| report.blocks.rotation.as_ref()?.by_entity.get(id));
        let n_series = entity_id
            .and_then(|id| report.blocks.series.as_ref()?.by_entity.get(id));
        let n_minions = entity_id
            .and_then(|id| report.blocks.minions.as_ref()?.by_entity.get(id));
        let team_id_for = |color: &str| -> u64 {
            detected_players.get(color).copied().unwrap_or_else(|| representative_team_id(color))
        };
        let act = activity.get(player_idx);
        // Real EI shape (verified against a real dps.report export,
        // axibridge's `test-fixtures/boon/20260117-181030.json`,
        // `players[0].statsAll[0]`): whole-fight/whole-phase aggregates —
        // downContribution, killed, downed, appliedCrowdControl,
        // appliedCrowdControlDuration — live under `statsAll[phase]`, not
        // `statsTargets`. We don't model phases, so this is a single-element
        // array standing in for "phase 0 == the whole fight", matching how
        // EI itself collapses to one phase for logs with no phase splits.
        // `appliedCrowdControlDuration` is real EI's own ms convention (no
        // /1000 — cross-checked: our computed `cc_duration_ms=50460` equals
        // the golden fixture's `squadAppliedCrowdControlDuration=50460`
        // exactly, unlike the stun-break duration below which EI reports in
        // seconds). Every field here is backed by a real computed metric —
        // `appliedCrowdControlDownContribution` /
        // `appliedCrowdControlDurationDownContribution` (CC's own down-contribution
        // split) exist in real EI but we don't compute that finer breakdown,
        // so they're intentionally omitted rather than faked.
        //
        // M11 Task 2: `downContribution` now maps to `p.downs_contribution.
        // damage` (the arcdps-methodology `damage_to_downs` value), NOT
        // EI's own down-contribution number — EI computes down-contribution
        // with a DIFFERENT algorithm BY DESIGN (see `axilog_core::analysis::
        // contribution`'s module doc: no EI golden exists to calibrate this
        // engine against, by design, since the whole point of this project's
        // founding differentiator is to diverge from EI's approximation and
        // match the real arcdps methodology instead). This mapping is a
        // best-effort "closest real EI field for this concept" placement,
        // not a parity claim — a consumer wanting EI's OWN algorithm's
        // number has no equivalent field in this adapter's output at all.
        let stats_all = json!([ {
            "downContribution": n_contribution.map_or(0, |c| c.downs_contribution.damage),
            "killed": n_damage.map_or(0, |d| d.kills_dealt),
            "downed": n_damage.map_or(0, |d| d.downs_dealt),
            "appliedCrowdControl": n_cc.map_or(0, |c| c.applied_total),
            "appliedCrowdControlDuration": n_cc.map_or(0, |c| c.applied_duration_ms),
            // M13 Task 3: outgoing hit-quality fields, mapped from
            // `p.hit_stats` (`HitStatsOut`, mirrors `axilog_core::
            // analysis::hit_stats::HitStats` field-for-field -- see that
            // module's doc comment for the exact per-field EI derivation/
            // citation, including WHY `criticalRate`/`flankingRate`/
            // `glanceRate`/`againstMovingRate` are plain COUNTS despite the
            // "Rate" naming, verbatim from real EI). Field names verified
            // against a real dps.report export
            // (`fixtures/wvw-small.ei.json`'s `players[].hitStats` sidecar,
            // itself a verbatim subset of a real `statsAll[0]`). Always
            // present (not gated) -- `hit_stats` is unconditionally
            // computed by `analyze()`, same "always-on" convention as the
            // down-contribution/CC fields above.
            "criticalRate": n_hit_stats.map_or(0, |h| h.crit_count),
            "criticalDmg": n_hit_stats.map_or(0, |h| h.crit_damage),
            "flankingRate": n_hit_stats.map_or(0, |h| h.flank_count),
            "glanceRate": n_hit_stats.map_or(0, |h| h.glance_count),
            "againstMovingRate": n_hit_stats.map_or(0, |h| h.moving_count),
            "connectedDamageCount": n_hit_stats.map_or(0, |h| h.connected_count),
            "connectedDmg": n_hit_stats.map_or(0, |h| h.connected_damage),
            "connectedDirectDamageCount": n_hit_stats.map_or(0, |h| h.direct_count),
            "connectedDirectDmg": n_hit_stats.map_or(0, |h| h.direct_damage),
            "connectedConditionCount": n_hit_stats.map_or(0, |h| h.condition_count),
            "connectedConditionDamage": n_hit_stats.map_or(0, |h| h.condition_damage),
            "critableDirectDamageCount": n_hit_stats.map_or(0, |h| h.critable_direct_count),
            "againstDownedCount": n_hit_stats.map_or(0, |h| h.against_downed_count),
            "againstDownedDamage": n_hit_stats.map_or(0, |h| h.against_downed_damage),
            "connectedLifeLeechCount": n_hit_stats.map_or(0, |h| h.life_leech_count),
            "connectedLifeLeechDamage": n_hit_stats.map_or(0, |h| h.life_leech_damage),
            "connectedPowerAbove90HPCount": n_hit_stats.map_or(0, |h| h.above90_power_count),
            "connectedPowerAbove90HPDamage": n_hit_stats.map_or(0, |h| h.above90_power_damage),
            "connectedConditionAbove90HPCount": n_hit_stats.map_or(0, |h| h.above90_condition_count),
            "connectedConditionAbove90HPDamage": n_hit_stats.map_or(0, |h| h.above90_condition_damage),
            // MSMALL item 3: the `JsonGameplayStatsAll` aftercast/interrupt
            // family, from `p.aftercast` (`AftercastOut`, mirroring
            // `axilog_core::analysis::rotation::AftercastStats` -- see that
            // struct's doc comment for the full
            // `GameplayStatistics.cs:81-99` transcription and the
            // `GetCastEvents` window filter that makes it exact).
            //
            // Field names and units from `JsonStatisticsBuilder.
            // BuildJsonGameplayStatsAll` (`GW2EIBuilders/JsonModels/
            // JsonActorUtilities/JsonStatisticsBuilder.cs:149-152`):
            //   Wasted     = gameStats.SkillAnimationInterruptedCount
            //   TimeWasted = gameStats.SkillAnimationInterruptedDuration
            //   Saved      = gameStats.SkillAnimationAfterCastInterruptedCount
            //   TimeSaved  = gameStats.SkillAnimationAfterCastInterruptedDuration
            // The two counts are plain ints; the two durations are SECONDS
            // (`Math.Round(ms / 1000.0, ParserHelper.TimeDigit)`, TimeDigit
            // = 3) -- `ei_time_secs`' exact convention.
            //
            // `wasted`/`timeWasted` here are CAST-INTERRUPT counters and
            // have nothing to do with the boon-generation `wasted` in
            // selfBuffs/groupBuffs/squadBuffs. Both names are real EI's.
            //
            // Calibrated on `fixtures/local/wvw-postrework.zevtc` against
            // that log's own EI export: all FOUR fields exact for all 44
            // players.
            "saved": n_rotation.map_or(0, |r| r.aftercast.saved_count),
            "timeSaved": ei_time_secs(n_rotation.map_or(0, |r| r.aftercast.saved_ms).max(0) as u64),
            "wasted": n_rotation.map_or(0, |r| r.aftercast.wasted_count),
            "timeWasted": ei_time_secs(n_rotation.map_or(0, |r| r.aftercast.wasted_ms).max(0) as u64)
        } ]);
        // Real EI's `statsTargets[targetIndex][phaseIndex]` carries a large
        // per-target breakdown (including its own per-target
        // downContribution/appliedCrowdControl split — see statsAll comment
        // above). We only compute one real per-target metric,
        // `damage.per_enemy` (damage dealt to that specific enemy), so that's
        // the only field we emit here — one entry per real `targets[]` enemy,
        // in the same order, `0` when this player dealt no damage to that
        // target. Everything else in real EI's per-target stats block would
        // have to be invented, so it's left out rather than faked.
        //
        // M10 Task 3 / MROSTER: reads `report.ei_targets` (the curated
        // GW2EI WvW `targets[]` roster -- enemy PLAYERS only, see that
        // field's doc comment for the `WvWLogic.cs` derivation), NOT
        // `report.enemies` (the native/HTML combat-participant list, which
        // is a different filter over the same `enc.enemies` and still
        // carries NPCs). `statsTargets[][]` is positionally keyed to
        // `targets[]` below, so every per-target array in this function
        // must be built off THIS one list and no other.
        //
        // MEIGAP Task 1d: the per-target OFFENSIVE SPLIT joins `totalDmg`
        // here, exactly when this player's native `per_target` block is
        // present (`--skill-damage`/SDK `skill_damage: true` -- the same
        // flag that gates `targetDamageDist` below, and the same
        // "presence, not a flag" convention every other opt-in block in
        // this function uses). Without it the row keeps today's
        // `totalDmg`-only shape: the split keys are OMITTED, never emitted
        // as zeros, so a consumer can still tell "not computed" from
        // "genuinely none" -- which is exactly what axibridge's own
        // `!sawTargetSplit` guard (`src/main/detailsProcessing.ts`) keys
        // off. The fields are -- `killed`, `downed`, `connectedDamageCount`,
        // `connectedDmg`, `againstDownedCount`, `interrupts` and
        // `downContribution`, from `p.per_target`
        // (`axilog_core::analysis::per_target`, whose module doc carries
        // the per-field `OffensiveStatistics` citation trail). Real EI
        // computes all of them from the SAME statistics class as
        // `statsAll[0]`, only with a non-null `target`
        // (`JsonPlayerBuilder.cs:122` -> `SingleActor.cs:680-691`), and
        // this reproduces that relationship exactly: each field is the
        // already-calibrated whole-fight pass' own predicate, restricted to
        // one enemy, so the per-target column sums back to its `statsAll`
        // counterpart by construction.
        //
        // `connectedHits` is NOT an EI field name -- axibridge reads it
        // through a `connectedHits ?? connectedDamageCount ?? hits` chain
        // (`computeFightDiffMode.ts:78-90`) and real EI only ever emits
        // `connectedDamageCount` (verified across the whole reference
        // export). So `connectedDamageCount` is what's emitted; inventing
        // an EI-shaped key EI itself never writes would be the wrong kind
        // of compatibility.
        //
        // `downContribution` is this project's arcdps-methodology number
        // (`p.per_target[].downs_contribution_damage`), NOT EI's own
        // 90%-to-downstate-window algorithm -- the identical deliberate
        // divergence, with the identical honesty caveat, that
        // `statsAll[0].downContribution` above already carries. It is the
        // one field in this block that is a "closest real EI field for
        // this concept" placement rather than a parity claim.
        //
        // Everything else real EI carries on `statsTargets[i][0]` (the
        // full connected*/critable*/flanking/glance/moving family, the
        // miss/evade/block/invuln outcomes, the per-target
        // appliedCrowdControl* split, `againstDownedDamage`) is still not
        // computed per-target here, and stays omitted rather than faked.
        let stats_targets: Vec<Value> = legacy.ei_targets.iter().map(|e| {
            let dmg = p.damage.per_enemy.iter().find(|pe| pe.enemy_id == e.id)
                .map(|pe| pe.total).unwrap_or(0);
            let mut row = json!({ "totalDmg": dmg });
            if let Some(split) = &p.per_target {
                let t = split.iter().find(|t| t.enemy_id == e.id);
                let obj = row.as_object_mut().expect("statsTargets row is an object");
                for (k, v) in [
                    ("killed", t.map(|t| t.killed).unwrap_or(0) as u64),
                    ("downed", t.map(|t| t.downed).unwrap_or(0) as u64),
                    ("connectedDamageCount", t.map(|t| t.connected_hits).unwrap_or(0) as u64),
                    ("connectedDmg", t.map(|t| t.connected_damage).unwrap_or(0)),
                    ("againstDownedCount", t.map(|t| t.against_downed_count).unwrap_or(0) as u64),
                    ("interrupts", t.map(|t| t.interrupts).unwrap_or(0) as u64),
                    ("downContribution", t.map(|t| t.downs_contribution_damage).unwrap_or(0)),
                ] {
                    obj.insert(k.to_string(), json!(v));
                }
            }
            json!([row])
        }).collect();
        let mut v = json!({
            "account": p.account,
            "character_name": p.character,
            // EI convention: `profession` is the elite-spec name when the
            // player has one active, else the base profession. `elite_spec` is
            // kept alongside for consumers that want the native split.
            "profession": if p.elite_spec.is_empty() { &p.profession } else { &p.elite_spec },
            "elite_spec": p.elite_spec,
            "teamID": team_id_for(&p.team),
            "group": p.subgroup,
            "notInSquad": !p.in_squad,
            "hasCommanderTag": p.commander,
            // `breakbarDamage` (MEIGAP2 row 6): GW2EI's `dpsAll[0].
            // breakbarDamage` -- the defiance-bar damage this player DEALT,
            // minion-inclusive, in GW2EI's own units (`BreakbarDamageEvent`
            // decodes the raw arcdps `value` as `Math.Round(value / 10.0,
            // 1)`, so the raw integer sum this project carries is divided
            // once, here, at the adapter boundary). Always-on: it is one
            // scalar per player, and the audit row it closes
            // (`docs/axilog-cutover-report.md:86`) is read unconditionally
            // (`packages/bridge-metrics/src/dashboardMetrics.ts:45`).
            // GW2EI's `actorBreakbarDamage` (the same sum WITHOUT the
            // minion fold) is not computed separately and is omitted
            // rather than faked with the folded number.
            "dpsAll": [ {
                "dps": n_damage.map_or(0.0, |d| d.dps).round() as i64,
                "damage": n_damage.map_or(0, |d| d.total),
                "breakbarDamage": ei_breakbar(n_damage.map_or(0, |d| d.breakbar_damage_dealt)),
            } ],
            "statsAll": stats_all,
            "statsTargets": stats_targets,
            "defenses": [ {
                "downCount": n_defenses.map_or(0, |d| d.downs_taken),
                "deadCount": n_defenses.map_or(0, |d| d.deaths),
                "damageTaken": n_damage.map_or(0, |d| d.taken),
                // M13 Task 3: hit-outcome + damage-taken breakdown fields,
                // mapped from `p.defenses` (`DefensesOut`, mirrors
                // `axilog_core::analysis::defenses::DefenseStats`
                // field-for-field -- see that module's doc comment for the
                // exact per-field EI derivation/citation). Field names
                // verified against a real dps.report export
                // (`fixtures/wvw-small.ei.json`'s `players[].defenses`
                // sidecar, itself a verbatim subset of a real
                // `defenses[0]`). Always present (not gated), same
                // always-on convention as `downCount`/`deadCount`/
                // `damageTaken` above.
                //
                // IMPORTANT -- `lifeLeechDamageTakenCount` intentionally
                // diverges from real EI: this emits OUR correct derived
                // value (`n_defenses.map_or(0, |d| d.life_leech_count)`), NOT a
                // reproduction of GW2EI's own real, verified counting bug
                // (`DefensePerTargetStatistics.cs`'s life-leech branch
                // increments the SUM field a second time instead of the
                // COUNT field, so real EI always reports 0 here even on a
                // fight with substantial nonzero `lifeLeechDamageTaken` --
                // see `axilog_core::analysis::defenses`'s module doc for
                // the full source-line citation). An exact-vs-real-EI diff
                // on this ONE field is therefore an intentional, documented
                // divergence -- axilog is more correct here, not less.
                // `crates/axilog-ei/tests/ei_golden.rs` calibrates every
                // other `defenses[0]` field exactly against the golden
                // fixture and asserts THIS one against the algebraically
                // derived TRUE reference instead of the fixture's raw
                // (buggy) value, mirroring `defenses_golden.rs`'s own
                // native-layer calibration.
                "blockedCount": n_defenses.map_or(0, |d| d.blocked_count),
                "evadedCount": n_defenses.map_or(0, |d| d.evaded_count),
                "dodgeCount": n_defenses.map_or(0, |d| d.dodge_count),
                "missedCount": n_defenses.map_or(0, |d| d.missed_count),
                "interruptedCount": n_defenses.map_or(0, |d| d.interrupted_count),
                "invulnedCount": n_defenses.map_or(0, |d| d.invulned_count),
                "strikeDamageTaken": n_defenses.map_or(0, |d| d.strike_damage),
                "strikeDamageTakenCount": n_defenses.map_or(0, |d| d.strike_count),
                "powerDamageTaken": n_defenses.map_or(0, |d| d.power_damage),
                "powerDamageTakenCount": n_defenses.map_or(0, |d| d.power_count),
                "conditionDamageTaken": n_defenses.map_or(0, |d| d.condition_damage),
                "conditionDamageTakenCount": n_defenses.map_or(0, |d| d.condition_count),
                "lifeLeechDamageTaken": n_defenses.map_or(0, |d| d.life_leech_damage),
                "lifeLeechDamageTakenCount": n_defenses.map_or(0, |d| d.life_leech_count),
                "damageBarrier": n_defenses.map_or(0, |d| d.barrier_damage),
                "damageBarrierCount": n_defenses.map_or(0, |d| d.barrier_count),
                // MEIGAP2 review: this was emitting RAW arcdps units, ten
                // times GW2EI's own number, and the milestone that added
                // the DEALT twin would otherwise have shipped the two
                // halves of the same quantity on different scales.
                // `DefensePerTargetStatistics.cs:143-148` accumulates
                // `brk.BreakbarDamage` -- the value `BreakbarDamageEvent`'s
                // ctor has ALREADY divided (`BreakbarDamage =
                // Math.Round(evtcItem.Value / 10.0, 1)`,
                // `ParsedData/CombatEvents/NonDamageEvents/
                // BreakbarDamageEvent.cs:8`) -- and rounds the sum to 1
                // decimal at `:148`. So the same `ei_breakbar` conversion
                // `dpsAll[0].breakbarDamage` uses applies here, and the
                // core keeps its raw integer sum on both sides.
                //
                // No calibration could catch this: `breakbarDamageTaken` is
                // 0 for every player in BOTH reference exports (neither a
                // WvW zerg capture records defiance-bar damage against
                // squad members), so this is a source-read fix, and it
                // changes the rendered document only on a log that has
                // squad-directed breakbar damage. `breakbarDamageTakenCount`
                // is a count and is untouched.
                "breakbarDamageTaken": ei_breakbar(n_defenses.map_or(0, |d| d.breakbar_damage)),
                "breakbarDamageTakenCount": n_defenses.map_or(0, |d| d.breakbar_count),
                // MEIGAP Task 1c: incoming CC + incoming boon strips, the
                // last four always-on `defenses[0]` fields axibridge reads.
                // Mapped from `p.defenses` like every field above -- see
                // `axilog_core::analysis::defenses::DefenseStats`'s
                // `received_cc_count`/`boon_strips_taken` doc comments for
                // the full GW2EI derivation (`JsonStatistics.cs:130-137,
                // 150-154`, `DefensePerTargetStatistics.cs:48-70,136-141,
                // 149`).
                //
                // `receivedCrowdControlDuration` stays in MILLISECONDS,
                // EI's own convention on this field (`CrowdControlEvent.
                // Duration = evtcItem.Value`, summed with no `/1000` at
                // `DefensePerTargetStatistics.cs:139`) -- the same ms
                // convention `appliedCrowdControlDuration` above already
                // uses, and unlike `support[0].removedStunDuration` below,
                // which EI really does report in seconds.
                //
                // `boonStripsTime` IS in seconds (`GetStripData`'s closing
                // `Math.Round(stripTime / 1000.0, ParserHelper.TimeDigit)`,
                // `:69`) -- BUT this emits the TRUE duration sum, not a
                // reproduction of GW2EI's own verified arithmetic bug on
                // that line's accumulator (`Math.Max(current + removed,
                // LogDuration)` where `Min` was intended, `:63`), which
                // inflates the exported number to roughly
                // `distinct_boons_stripped * logDuration`. Same
                // "axilog is correct here, not less" convention as
                // `lifeLeechDamageTakenCount` above; see
                // `DefenseStats::boon_strips_taken_duration_ms`'s doc
                // comment for the measured proof on the reference export,
                // and `crates/axilog-ei/tests/meigap_ei_golden.rs` for the
                // calibration, which reconstructs EI's formula from our own
                // per-boon strip detail (pinning the distinct-boon set, the
                // per-boon removal count and every removal after the first;
                // EI's `Max` swallows the first one's duration).
                // `TimeDigit` is 3, matching the export's own precision.
                "receivedCrowdControl": n_defenses.map_or(0, |d| d.received_cc_count),
                "receivedCrowdControlDuration": n_defenses.map_or(0, |d| d.received_cc_duration_ms),
                "boonStrips": n_defenses.map_or(0, |d| d.boon_strips_taken),
                "boonStripsTime": ei_time_secs(n_defenses.map_or(0, |d| d.boon_strips_taken_duration_ms))
            } ],
            // EI places stun-break stats under `support`, not `defenses` — verified
            // against GW2EI's `SupportAllStatistics` (StunBreakCount /
            // RemovedStunDuration) and the real dps.report EI JSON for the golden
            // WvW fixture (`support[0].stunBreak` / `support[0].removedStunDuration`,
            // not `defenses[0]`). `removedStunDuration` is EI's convention of
            // seconds (our native schema tracks whole ms). M3 Task 5: extended with
            // the condi-cleanse/boon-strip/resurrect counts (M3 Task 3) — field
            // names verified against a real dps.report EI export
            // (axibridge's `test-fixtures/boon/20260117-181030.json`,
            // `players[0].support[0]`: `condiCleanse`, `condiCleanseSelf`,
            // `boonStrips`, `resurrects`). The `*Time`/`boonStripDownContribution*`
            // sibling fields real EI also carries aren't computed here, so they're
            // omitted rather than faked (same convention as `statsTargets` above).
            "support": [ {
                "stunBreak": n_cc.map_or(0, |c| c.stun_breaks),
                "removedStunDuration": n_cc.map_or(0, |c| c.removed_stun_duration_ms) as f64 / 1000.0,
                "condiCleanse": n_support.map_or(0, |s| s.cleanses),
                "condiCleanseSelf": n_support.map_or(0, |s| s.cleanses_self),
                "boonStrips": n_support.map_or(0, |s| s.strips),
                // MEIGAP Task 3e: the outgoing twin of
                // `defenses[0].boonStripsTime`, and the same deliberate,
                // calibrated divergence -- axilog emits the TRUE sum in
                // seconds, GW2EI's own export carries the value its buggy
                // `Math.Max(foeTime + RemovedDuration, LogDuration)`
                // accumulator produces
                // (`SupportPerAllyStatistics.cs`). See
                // `axilog_core::analysis::support::SupportMetrics::
                // strips_duration_ms` for the full transcription and what
                // the calibration's reconstruction pins.
                "boonStripsTime": ei_time_secs(n_support.map_or(0, |s| s.strips_duration_ms)),
                "resurrects": n_support.map_or(0, |s| s.resurrects)
            } ],
            // `buffUptimes[]` (M3 Task 5): one entry per tracked boon (the 12 in
            // `BOON_IDS`), in that table's order. Field meanings verified against
            // GW2EI's own `uptime`/`presence` semantics (see
            // `axilog_core::analysis::buffs::uptime`'s module doc, cross-checked
            // against the same real dps.report export): for DURATION-type boons
            // `uptime` is our `presence_pct` and `presence` is always 0 (EI never
            // populates it for that branch); for INTENSITY-type boons (Might,
            // Stability) `uptime` is our raw `avg_stacks` (no `*100`) and
            // `presence` is our `presence_pct`. `generated` in real EI is a full
            // per-source-character-name ms/pct map; we only compute a self/group/
            // squad ROLLUP (M3 Task 4), not a per-character breakdown, so the only
            // entry we can honestly attribute to a specific character name is the
            // player's own self-generation — emitted as `{ <own character>:
            // self_pct }` rather than fabricating group/squad members' names.
            //
            // MEIGAP Task 1b: `states` and `statesPerSource` join each
            // entry exactly when `EiInputs::boon_states` was supplied (see
            // that field's doc comment for GW2EI's own
            // `RawFormatTimelineArrays` gate, which it mirrors). Both are
            // `[[time_ms_from_log_start, stackCount], ...]`;
            // `statesPerSource` keys by source CHARACTER name, with
            // GW2EI's own `"UNKNOWN"` placeholder for an unresolved source
            // -- see `axilog_core::analysis::buffs::states`'s module doc
            // for the full shape/citation trail, including why a leading
            // `[0, 0]` pair is always present.
            "buffUptimes": p.boons.iter().map(|b| {
                let (uptime, presence) = match b.avg_stacks {
                    Some(avg) => (avg, b.presence_pct), // intensity boon
                    None => (b.presence_pct, 0.0),      // duration boon
                };
                let mut entry = json!({
                    "id": b.id,
                    "buffData": [ {
                        "uptime": uptime,
                        "presence": presence,
                        "generated": { &p.character: b.generation.self_pct }
                    } ]
                });
                if let Some(bs) = boon_states {
                    let obj = entry.as_object_mut().expect("buffUptimes entry is an object");
                    let key = (p.agent_addr, b.id);
                    obj.insert(
                        "states".to_string(),
                        ei_states_json(bs.total.get(&key).map(|v| v.as_slice()).unwrap_or(&[])),
                    );
                    let per_source: BTreeMap<&str, Value> = bs
                        .per_source
                        .get(&key)
                        .into_iter()
                        .flatten()
                        .map(|(name, states)| (name.as_str(), ei_states_json(states)))
                        .collect();
                    obj.insert("statesPerSource".to_string(), json!(per_source));
                }
                entry
            }).collect::<Vec<_>>(),
            // `selfBuffs`/`groupBuffs`/`squadBuffs` (MEIGAP Task 1a): the
            // boon-generation ATTRIBUTION arrays, i.e. "how much boon-time
            // did THIS player generate, for himself / for his subgroup /
            // for the squad".
            //
            // Shape (`GW2EIJSON/JsonActors/JsonPlayer.cs:233,239,251` ->
            // `GW2EIJSON/JsonActorUtilities/JsonPlayerUtilities/
            // JsonPlayerBuffsGeneration.cs:53,60`): `[{ id, buffData: [
            // {...} ] }]`, the inner array being per-phase (one element
            // here -- this project's one-phase convention, same as
            // `statsAll`/`totalDamageDist`).
            //
            // Scope, verified against `GW2EIEvtcParser/EIData/Actors/
            // Player.cs:58-69`: `Self` -> `BuffStatistics.GetBuffsForSelf`
            // (this player as both source and target), `Group` -> every
            // OTHER player with the same subgroup, `Squad` -> every OTHER
            // player in `log.PlayerList`. That is exactly the tripartite
            // split `axilog_core::analysis::buffs::generation`'s
            // `GenerationStats` already computes and M3 Task 4 calibrated
            // (see that module's doc comment for the per-scope citation
            // trail and the ms->percent/avg-stacks scaling, which matches
            // `BuffStatistics.cs:116-121`/`:135-141` term for term:
            // duration boons `*100`, intensity boons raw average stacks).
            //
            // Emitted fields: `generation` and `wasted` (MSMALL item 2 added
            // the latter -- the one axibridge also reads). Real EI's
            // remaining sibling fields on the same object --
            // `generationPresence`, `overstack` (which is really
            // overstack+generation, `BuffStatistics.cs:117`),
            // `unknownExtended`, `byExtension`, `extended` -- come from the
            // simulator's OVERSTACK/EXTENSION channels
            // (`GW2EIEvtcParser/EIData/Buffs/BuffSimulators/SimulationItem.cs:81-115`,
            // `BuffSimulatorNoID/BuffSimulator.cs:67,117,122`), which this
            // project's simulator still does not model. They're omitted
            // rather than faked, the same "don't fake absent data"
            // convention `statsTargets`/`support`/`extHealingStats` above
            // already follow.
            //
            // Because `wasted` is now populated, it also participates in the
            // group/squad id-set filter -- a waste-only source is a real EI
            // row. See `buff_generation_json`'s doc comment for the
            // `HasSrc`/`AddWaste` citation and the measured cell count.
            //
            // Id sets, following EI's own two DIFFERENT rules:
            //
            // - `selfBuffs` carries every id `buffUptimes` carries, zeros
            //   included. Verified on the reference export: its `selfBuffs`
            //   id list is character-for-character its `buffUptimes` id
            //   list, 43 of 43 on every player, and its first entry there
            //   really does read `"generation": 0`. This emits all 12
            //   tracked boons for the same reason -- `buffUptimes` above
            //   emits all 12. (EI's 43 ids against this project's 12 is the
            //   tracked-boon SCOPE difference the README already records --
            //   these arrays are a 12-of-43 SUBSET, not a parity claim --
            //   not a difference in this rule.)
            // - `groupBuffs`/`squadBuffs` are filtered to buffs this player
            //   is a recorded SOURCE for: `BuffStatistics.cs:66,100`'s
            //   `hasGeneration`, i.e. `buffDistribution.HasSrc(boon.ID,
            //   srcAgentItem)` on at least one target in scope. A source
            //   with nonzero generated-ms on some target is exactly such a
            //   source, so `> 0` reproduces that filter. It is marginally
            //   NARROWER than `HasSrc`, which is also true for a source
            //   that contributed only overstack/waste/extension and no held
            //   time -- an all-zero row on the one field this adapter
            //   emits, so dropping it loses nothing.
            //
            // Emitting all 12 in all THREE arrays was measured first and
            // rejected: the zero rows carry no information and cost 39.4%
            // of these arrays' bytes (see the MEIGAP Task 1 report's
            // always-on size decision).
            "selfBuffs": buff_generation_json(&p.boons, |g| g.self_pct, |g| g.self_wasted, true),
            "groupBuffs": buff_generation_json(&p.boons, |g| g.group_pct, |g| g.group_wasted, false),
            "squadBuffs": buff_generation_json(&p.boons, |g| g.squad_pct, |g| g.squad_wasted, false)
        });
        // `activeTimes`/`combatReplayData` (M11 Task 3): unlike every other
        // block on this player, these are ALWAYS present -- not gated on
        // `--replay`/SDK `replay: true` (the module doc on
        // `axilog_core::analysis::replay::build_activity_intervals` explains
        // why: down/dead intervals and first/last-aware bounds are cheap,
        // only the downsampled `positions[]` track is expensive, and that
        // stays absent here regardless -- a consumer wanting the position
        // MAP (not just down/dead-derived features) still needs `--replay`'s
        // native `replay` block, deferred to M15 for ei-json specifically).
        // `activeTimes` real EI shape: a single-element array (this
        // project's one-phase-only convention, matching `statsAll`/
        // `extHealingStats` above) holding
        // `SingleActor.GetActiveDuration(log, 0, durationMS)` --
        // `ActivityIntervals::active_ms`'s doc comment has the full GW2EI
        // source citation and the real-golden verification that down time
        // is NOT subtracted (only dead time is). `combatReplayData.start`/
        // `.end` mirror GW2EI's `AgentItem.FirstAware`/`LastAware`;
        // `.down`/`.dead` are `[[start_ms, end_ms], ...]` arrays verified
        // byte-exact against the committed EI golden
        // (`crates/axilog-ei/tests/ei_golden.rs`).
        //
        // M15 Task 3: `.positions`/`.orientations`/`.dc`/`.iconURL` join
        // this object when -- and only when -- `replay` was requested (see
        // `to_ei_json`'s `replay` argument). The four M11 fields above stay
        // ALWAYS-ON and are still sourced from `activity`, unchanged, in
        // both modes: `ei_golden.rs`'s
        // `ei_json_replay_fields_do_not_disturb_the_always_on_surface`
        // asserts the two objects are byte-identical apart from the four
        // added keys.
        {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            let (active_ms, start_ms, end_ms, down, dead) = match act {
                Some(a) => (
                    a.active_ms(),
                    a.start_ms,
                    a.end_ms,
                    a.down_intervals.iter().map(interval_json).collect::<Vec<_>>(),
                    a.dead_intervals.iter().map(interval_json).collect::<Vec<_>>(),
                ),
                None => (0, 0, 0, Vec::new(), Vec::new()),
            };
            obj.insert("activeTimes".to_string(), json!([active_ms]));
            let mut crd = json!({
                "start": start_ms,
                "end": end_ms,
                "down": down,
                "dead": dead
            });
            if let Some(track) = replay.map(|r| &r.tracks[player_idx]) {
                let crd = crd.as_object_mut().expect("combatReplayData is a JSON object");
                crd.insert("positions".to_string(), ei_positions_json(&track.positions));
                crd.insert("orientations".to_string(), ei_orientations_json(&track.orientations));
                crd.insert("dc".to_string(), ei_intervals_json(&track.dc));
                // GW2EI's `SingleActorCombatReplayDescription.Img =
                // actor.GetIcon(true)`, i.e. always the BASE-resolution
                // profession/elite-spec icon -- see
                // `axilog_core::icons`'s module doc for the full
                // `GetIcon`/`GetProfIcon`/`BaseResProfIcons` chain and the
                // 16-spec exact calibration against the local reference.
                crd.insert(
                    "iconURL".to_string(),
                    json!(prof_icon_url(&p.profession, &p.elite_spec)),
                );
            }
            obj.insert("combatReplayData".to_string(), crd);
        }
        // `totalDamageDist`/`targetDamageDist`/`totalDamageTaken` (M12,
        // Task 3): only present when this player's native `skill_damage`
        // block is present (`--skill-damage`/SDK `skill_damage: true` was
        // requested when the `Report` was built --
        // `axilog_schema::PlayerOut::skill_damage`'s doc comment) -- keyed
        // off THAT presence, not a separate flag threaded through
        // `to_ei_json` itself (the M12 Task 3 brief: "key off presence, not
        // a flag"), so a `Report` built without `--skill-damage` gets these
        // three arrays OMITTED entirely, not emitted empty. Real EI shape
        // verified against axibridge's `test-fixtures/boon/
        // 20260117-181030.json`: `totalDamageDist`/`totalDamageTaken` are
        // `[phase][skillEntry]` (a single-element phase array wrapping the
        // skill list -- this project's one-phase convention, matching
        // `statsAll`/`extHealingStats` elsewhere in this fn);
        // `targetDamageDist` is `[targetIndex][phase][skillEntry]`,
        // positionally keyed to `targets[]` (built from
        // `report.ei_targets`, the same curated roster/positional
        // convention `statsTargets` above already uses) -- a target this
        // player never damaged gets an empty skill list (`[[]]`), not an
        // absent entry, matching real EI's own always-present-per-target
        // shape.
        if let Some(sd) = &p.skill_damage {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            // MEIGAP2 row 1: the outcome columns ride the same
            // `--skill-damage` gate the distributions themselves do, and
            // are joined per player by representative addr. See
            // `dist_rows_ei_json` for the union/`hits`/`downContribution`
            // rules.
            let po = dist_outcomes.and_then(|m| m.get(&p.agent_addr));
            obj.insert(
                "totalDamageDist".to_string(),
                json!([dist_rows_ei_json(
                    &sd.outgoing,
                    po.map(|o| o.outgoing.as_slice()),
                    Some(&p.downs_contribution_per_skill),
                )]),
            );
            obj.insert(
                "totalDamageTaken".to_string(),
                json!([dist_rows_ei_json(&sd.taken, po.map(|o| o.taken.as_slice()), None)]),
            );
            let target_dist: Vec<Value> = legacy
                .ei_targets
                .iter()
                .map(|e| {
                    let skills = sd
                        .per_target
                        .iter()
                        .find(|t| t.enemy_id == e.id)
                        .map(|t| t.skills.iter().map(skill_entry_ei_json).collect::<Vec<_>>())
                        .unwrap_or_default();
                    json!([skills])
                })
                .collect();
            obj.insert("targetDamageDist".to_string(), json!(target_dist));
        }
        // `damage1S`/`damageTaken1S`/`targetDamage1S`/`dpsTargets` (M12,
        // Task 3): only present when this player's native `per_second`
        // block is present (`--timeseries`/SDK `timeseries: true` --
        // `axilog_schema::PlayerOut::per_second`'s doc comment); `dpsTargets`
        // is gated by that SAME `p.per_second.is_some()` check rather than
        // its own `p.dps_targets.is_empty()` check -- an empty `dps_targets`
        // Vec can't distinguish "not requested" from "requested, but this
        // player never damaged any enemy", while `axilog_schema::
        // build_report` populates BOTH `per_second` and `dps_targets` off
        // the identical `include_timeseries` bool (see that fn's doc
        // comment), so keying off `per_second`'s presence is the correct
        // "presence, not a flag" signal for both. Real EI shape (same
        // fixture): `damage1S`/`damageTaken1S` are `[phase][second]`
        // (single-element phase array wrapping the per-second numbers --
        // this project's own `per_second.damage`/`damage_taken` are ALREADY
        // cumulative running totals by construction, see
        // `axilog_core::analysis::timeseries`'s module doc, so no extra
        // transform is needed here, just the EI phase-array wrapper);
        // `targetDamage1S` is `[targetIndex][phase][second]`, `dpsTargets`
        // is `[targetIndex][phase]{dps, damage}` -- both positionally keyed
        // to `targets[]`/`report.ei_targets`, same convention
        // `targetDamageDist` above uses. A target this player never damaged
        // gets an all-zero series (length matching this player's own
        // `per_second.damage`) / a `{dps: 0, damage: 0}` entry, not an
        // absent one. `dps` is rounded to the nearest integer, matching
        // `dpsAll[0].dps`'s own convention above (real EI's own
        // `dpsTargets[][].dps` is likewise an integer on the source
        // fixture) -- only `dps`/`damage` are emitted, real EI's many other
        // `dpsTargets[][]` fields (`condiDps`, `powerDps`, `breakbarDamage`,
        // the `actor*` duplicates, ...) aren't computed here, omitted
        // rather than faked.
        if let Some(ps) = &p.per_second {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            obj.insert("damage1S".to_string(), json!([ps.damage]));
            obj.insert("damageTaken1S".to_string(), json!([ps.damage_taken]));
            // MEIGAP Task 2a: the POWER half of the same two families.
            // GW2EI emits `powerDamageTaken1S` beside `damageTaken1S` from
            // the identical `GetDamageTakenGraph` call with
            // `DamageType.Power` instead of `.All`
            // (`JsonPlayerBuilder.cs:76-77`), and `targetPowerDamage1S`
            // beside `targetDamage1S` likewise (`:99-100`). POWER is "not
            // `ConditionDamageBased`" (`Actor.cs:449-451`) -- strike AND
            // life-leech AND the non-catalogued `buff == 1` bucket, NOT
            // `strike + life_leech`; see `axilog_core::analysis::
            // timeseries`'s POWER-split section. `conditionDamageTaken1S`/
            // `targetConditionDamage1S` (the complements EI also emits) are
            // deliberately omitted: no axibridge reader touches them.
            obj.insert("powerDamageTaken1S".to_string(), json!([ps.power_damage_taken]));
            let buckets = ps.damage.len();
            let target_damage_1s: Vec<Value> = legacy
                .ei_targets
                .iter()
                .map(|e| {
                    let series = ps
                        .per_target
                        .iter()
                        .find(|t| t.enemy_id == e.id)
                        .map(|t| t.damage.clone())
                        .unwrap_or_else(|| vec![0u64; buckets]);
                    json!([series])
                })
                .collect();
            obj.insert("targetDamage1S".to_string(), json!(target_damage_1s));
            let target_power_damage_1s: Vec<Value> = legacy
                .ei_targets
                .iter()
                .map(|e| {
                    let series = ps
                        .per_target
                        .iter()
                        .find(|t| t.enemy_id == e.id)
                        .map(|t| t.power_damage.clone())
                        .unwrap_or_else(|| vec![0u64; buckets]);
                    json!([series])
                })
                .collect();
            obj.insert("targetPowerDamage1S".to_string(), json!(target_power_damage_1s));

            let dps_targets: Vec<Value> = legacy
                .ei_targets
                .iter()
                .map(|e| match p.dps_targets.iter().find(|d| d.enemy_id == e.id) {
                    Some(d) => json!([ { "dps": d.dps.round() as i64, "damage": d.damage } ]),
                    None => json!([ { "dps": 0, "damage": 0 } ]),
                })
                .collect();
            obj.insert("dpsTargets".to_string(), json!(dps_targets));
        }
        // `guildID` (MEIGAP Task 3c): GW2EI's own field, decoded from the
        // `CBTS_GUILD` statechange -- see `axilog_core::wvw::guilds` for
        // the byte permutation and for why this needs no GW2-API lookup.
        // Inserted conditionally rather than as a `json!` key so a log
        // without guild rows carries no `"guildID": null` noise; axibridge
        // guards with `typeof player?.guildID === 'string'`
        // (`src/shared/squadGuilds.ts:18`) either way.
        // `instanceID` (MEIGAP2 row 3): GW2EI's `JsonActor.InstanceID`,
        // `jsonActor.InstanceID = actor.InstID` (`JsonActorBuilder.cs:31`)
        // -- always emitted there, and always-on here too (one `u16` per
        // player). Inserted conditionally so an agent with no instid
        // registration at all is ABSENT rather than reported as instid `0`,
        // which axibridge would otherwise treat as a real identity
        // (`computePlayerAggregation.ts:536-539` keeps it only when finite
        // and `> 0`, so an absent field and a `0` behave the same there --
        // absence is the honest encoding of "unknown").
        // `boonsStates` (MEIGAP2 row 4): GW2EI's `[[time_ms, number of
        // boons present], ...]`, gated on `--timeseries` exactly as the
        // per-boon `buffUptimes[].states` above are (both sit inside
        // `if (settings.RawFormatTimelineArrays)`,
        // `JsonActorBuilder.cs:90-98`). Derived from the very timelines
        // that field publishes -- see
        // `axilog_core::analysis::buffs::states::boon_count_states` for the
        // presence-not-stacks rule and its `MergePresenceInto` citation.
        //
        // The consumer reduces it to one scalar (`boonsAppliedCount`, the
        // sum of the series' positive deltas) in main-process pruning and
        // then drops the array (`axibridge src/main/detailsProcessing.ts:
        // 128-142,167-170`), which is why the array itself is fine to keep
        // behind the timeline gate.
        if let Some(bs) = boon_states {
            let states = axilog_core::analysis::buffs::states::boon_count_states(bs, p.agent_addr);
            if !states.is_empty() {
                let obj = v.as_object_mut().expect("player value is always a JSON object");
                obj.insert("boonsStates".to_string(), ei_states_json(&states));
            }
        }
        if let Some(instid) = p.instid {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            obj.insert("instanceID".to_string(), Value::from(instid));
        }
        // `healthPercents` (MEIGAP2 row 2): GW2EI's `[[time_ms, percent],
        // ...]` step function, gated on `--timeseries` -- its own
        // `RawFormatTimelineArrays` gate (`JsonActorBuilder.cs:90-100`).
        // See `axilog_core::analysis::health::ei_health_percents` for the
        // `ListFromStates` transcription. Emitted even when empty, matching
        // GW2EI's own always-a-list shape for a tracked actor.
        //
        // Task 6: read from `blocks.series`, whose per-entity rows exist
        // exactly when `--timeseries` was requested -- the same condition
        // that used to make `EiInputs::health_percents` `Some`. The field
        // is itself an `Option` (see its doc comment): a player who never
        // emitted a `HEALTH_UPDATE` is absent from the pass's map and gets
        // no key here, exactly as when this read the side channel.
        if let Some(series) = n_series.and_then(|s| s.health_percents.as_ref()) {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            obj.insert(
                "healthPercents".to_string(),
                Value::Array(
                    series
                        .iter()
                        .map(|&(t, pct)| json!([t, ei_double(pct)]))
                        .collect(),
                ),
            );
        }
        if let Some(g) = &p.guild_id {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            obj.insert("guildID".to_string(), Value::from(g.clone()));
        }
        // `extHealingStats`/`extBarrierStats` (M10 Task 1): only when this
        // log carries the healing extension at all (`p.healing` is `None`
        // otherwise -- `axilog_schema::PlayerOut.healing`'s doc comment).
        // Real EI shape (verified against a real dps.report export,
        // axibridge's `test-fixtures/boon/20260117-181030.json`,
        // `players[0].extHealingStats.outgoingHealing[0]`/
        // `extBarrierStats.outgoingBarrier[0]`): both are single-phase
        // arrays, same "one array standing in for phase 0 == the whole
        // fight" convention `statsAll` above already uses. Only the exact-
        // named real EI fields we actually compute are emitted --
        // `healing`/`downedHealing`/`hps`/`downedHps` (outgoing, target=
        // null, i.e. every friendly-directed heal including self, per
        // `axilog_core::analysis::healing`'s module doc) and `barrier`/
        // `bps`. Real EI's `healingPowerHealing`/`conversionHealing`/
        // `hybridHealing` (skill-type breakdown), `outgoingHealingAllies`
        // (per-friendly array), `incomingHealing`, and every `*1S`/`*Dist`
        // field are NOT computed by this project (native-only
        // `healing_out_self`/`healing_out_allies` cover the self/ally
        // split instead -- see the native schema's `players[].healing`,
        // which has no direct EI-field-name equivalent to reuse here) --
        // omitted rather than faked, same convention as `statsTargets`/
        // `support` above.
        //
        // MEIGAP Task 3a fills in three of those gaps --
        // `outgoingHealingAllies`, `totalHealingDist`, `healing1S` (and
        // the two barrier twins) -- from `EiInputs::healing_detail`. The
        // rest of the sentence above still stands: the skill-type
        // breakdown, `incomingHealing` and every `allied*`/`*Received1S`
        // array remain uncomputed and unemitted.
        if let Some(h) = &p.healing {
            let detail = healing_detail.and_then(|d| d.get(player_idx));
            let mut healing = json!({
                "outgoingHealing": [ {
                    "healing": h.healing_out_total,
                    "hps": (h.healing_out_total as f64 / duration_secs).round() as i64,
                    "downedHealing": h.downed_healing_out,
                    "downedHps": (h.downed_healing_out as f64 / duration_secs).round() as i64,
                } ]
            });
            let mut barrier = json!({
                "outgoingBarrier": [ {
                    "barrier": h.barrier_out,
                    "bps": (h.barrier_out as f64 / duration_secs).round() as i64,
                } ]
            });
            if let Some(d) = detail {
                let ho = healing.as_object_mut().expect("object literal");
                let bo = barrier.as_object_mut().expect("object literal");
                // `outgoingHealingAllies[allyIndex][phase]` /
                // `outgoingBarrierAllies[allyIndex][phase]`: one row per
                // `players[]` entry, in that order, single-phase like every
                // other array in this document.
                //
                // GW2EI emits both unconditionally
                // (`EXTJsonPlayerHealingStatsBuilder.cs:73` sits outside
                // every `RawFormatTimelineArrays` block). They ride
                // `--skill-damage` here purely for PAYLOAD -- measured
                // always-on on `fixtures/wvw-small.anon.zevtc` they grow
                // the flagless compact ei-json 262,226 -> 356,662 bytes
                // (+36.0%), past the ~30% band every always-on block in
                // this schema has been held to, and they grow
                // QUADRATICALLY in squad size (41x41 cells here, 48x48 on
                // the real capture). Same treatment, same flag, and the
                // same Task-2c precedent as `targets[].totalDamageDist`,
                // which GW2EI also emits unconditionally; axibridge
                // hardcodes the flag to `true`, so the read surface is
                // unchanged.
                if healing_dist {
                    ho.insert(
                        "outgoingHealingAllies".to_string(),
                        Value::Array(
                            d.ally_healing
                                .iter()
                                .map(|c| {
                                    json!([ { "healing": c.healing, "downedHealing": c.downed_healing } ])
                                })
                                .collect(),
                        ),
                    );
                    bo.insert(
                        "outgoingBarrierAllies".to_string(),
                        Value::Array(
                            d.ally_barrier.iter().map(|&b| json!([ { "barrier": b } ])).collect(),
                        ),
                    );
                    // Two shape divergences from GW2EI, both deliberate and
                    // both invisible to an id-keyed consumer:
                    //
                    // 1. **Row ORDER.** GW2EI emits dist rows in
                    //    `GroupBy(x => x.SkillID)` order, i.e. first-event
                    //    order; these are sorted by skill id ascending (the
                    //    `BTreeMap` the pass accumulates into). Every reader
                    //    of this array in axibridge keys by `entry.id`
                    //    (`computePlayerAggregation.ts:1075-1097`), and so
                    //    does the calibration, so nothing observes the
                    //    difference -- but a byte-diff against a real export
                    //    would.
                    // 2. **Indirect ids are not added to `buffMap`.**
                    //    `BuildHealingDist` routes an `IndirectHealing` row's
                    //    id into `buffMap` rather than `skillMap`, so EI's
                    //    consumers can name it. This adapter's `buffMap` is
                    //    the 12 tracked boons (plus Task 2d's conditions),
                    //    and a healing-over-time id is neither -- so an
                    //    indirect row here carries a correct
                    //    `indirectHealing: true` and an id that resolves in
                    //    neither map. axibridge's `resolveSkillMeta` falls
                    //    back to `"Skill <id>"`, which is the same fallback
                    //    the always-on `skillMap` already documents for ids
                    //    whose name this project cannot resolve.
                    ho.insert(
                        "totalHealingDist".to_string(),
                        json!([ heal_dist_json(&d.healing_dist, "totalHealing", true) ]),
                    );
                    bo.insert(
                        "totalBarrierDist".to_string(),
                        json!([ heal_dist_json(&d.barrier_dist, "totalBarrier", false) ]),
                    );
                }
                if healing_series {
                    ho.insert("healing1S".to_string(), json!([ d.healing_1s ]));
                }
            }
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            obj.insert("extHealingStats".to_string(), healing);
            obj.insert("extBarrierStats".to_string(), barrier);
        }
        // `minions[]` (MEIGAP Task 3b): emitted only under
        // `--skill-damage` (the caller supplies `EiInputs::minions` exactly
        // then), and only for a player that actually has minions -- GW2EI
        // omits the key entirely for a player with none, and axibridge's
        // readers both start with an `Array.isArray(...) ? ... : []` guard,
        // so an empty array and an absent key are equivalent to it. Absent
        // is the smaller and the more faithful of the two.
        //
        // Task 6: read from `blocks.minions`. The block omits a player
        // with no minions rather than storing an empty vec, so the row's
        // mere presence is the condition the `filter(|g| !g.is_empty())`
        // used to express.
        if let Some(groups) = n_minions {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            obj.insert(
                "minions".to_string(),
                Value::Array(
                    groups
                        .iter()
                        .map(|g| {
                            // Identity comes back through `catalogs.
                            // minions` -- no block in the native format
                            // inlines a name, so the block carries only
                            // the join key.
                            let ident = report.catalogs.minions.get(&g.minion_id);
                            json!({
                                "id": ident.map_or(0, |m| m.species_id),
                                "name": ident.map_or("", |m| m.name.as_str()),
                                // `taken` is a `BTreeMap` keyed by skill
                                // id, which iterates in the same ascending
                                // order the pass's sorted vec had, so this
                                // array's order is unchanged.
                                "totalDamageTakenDist": [ g.taken.iter().map(|(skill_id, r)| json!({
                                    "id": skill_id,
                                    "totalDamage": r.total,
                                    "hits": r.hits,
                                    "connectedHits": r.connected_hits,
                                    "min": r.min,
                                    "max": r.max,
                                    "blocked": r.blocked,
                                    "evaded": r.evaded,
                                    "glance": r.glance,
                                    "missed": r.missed,
                                    "invulned": r.invulned,
                                    "interrupted": r.interrupted,
                                    "indirectDamage": r.indirect,
                                })).collect::<Vec<_>>() ]
                            })
                        })
                        .collect(),
                ),
            );
        }
        // `rotation[]` (M14, Task 3): only present when this player's
        // native `rotation` block is present (`--rotation`/SDK
        // `rotation: true` was requested when the `Report` was built --
        // `axilog_schema::PlayerOut::rotation`'s doc comment) -- keyed off
        // THAT presence, not a separate flag threaded through `to_ei_json`
        // itself, same "presence, not a flag" convention `skill_damage`'s
        // `totalDamageDist`/`per_second`'s `damage1S` mappings above already
        // establish. Real EI shape (verified against a real dps.report
        // export, axibridge's `test-fixtures/boon/20260117-181030.json`,
        // `players[0].rotation[0]`): a flat array of `{ id, skills: [ {
        // castTime, duration, timeGained, quickness } ] }`, one entry per
        // skill id this player cast -- NOT wrapped in a phase array (unlike
        // `statsAll`/`totalDamageDist`/etc above, real EI's own
        // `rotation[]` has no phase dimension at all). Field-for-field
        // straight copy of `axilog_schema::CastOut`/`SkillRotationOut`
        // (themselves a mirror of `axilog_core::analysis::rotation::Cast`/
        // `SkillRotation` -- see that module's doc comment for the full
        // GW2EI `AnimatedCastEvent` derivation this reproduces, and its
        // documented `InstantCastEvent`-pipeline scope gap, which applies
        // here identically since this is a direct copy of the same
        // already-computed data).
        if let Some(rotation) = &p.rotation {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            let rotation_json: Vec<Value> = rotation
                .iter()
                .map(|sr| {
                    json!({
                        "id": sr.skill_id,
                        "skills": sr.casts.iter().map(|c| json!({
                            "castTime": c.cast_time_ms,
                            "duration": c.duration_ms,
                            "timeGained": c.time_gained_ms,
                            "quickness": c.quickness,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            obj.insert("rotation".to_string(), json!(rotation_json));
        }
        // `damageModifiers` / `incomingDamageModifiers` /
        // `damageModifiersTarget` / `incomingDamageModifiersTarget` (M16,
        // Task 3): all four, together, exactly when `EiInputs::modifiers`
        // is present -- same "presence, not a flag" convention as
        // `rotation`/`totalDamageDist` above.
        //
        // Real EI shape (`JsonDamageModifierDataBuilder.cs:38-160`,
        // cross-checked field-for-field against the reference WvW export):
        //
        // - the whole-fight pair is a flat `[{ id, damageModifiers: [ ... ] }]`,
        //   the inner array being per-phase (one element here, see
        //   `ei_damage_mod_rows`);
        // - the Target pair is `[targetIndex][]` -- one entry per
        //   `log.LogData.Logic.Targets`, i.e. positionally keyed to
        //   `targets[]`, with `[]` for a target this player never
        //   exchanged a qualifying hit with. **Verified non-empty in WvW,
        //   not assumed:** the reference export's first player has 22 of 57
        //   outgoing and 14 of 57 incoming target slots populated, so
        //   emitting the empty shape everywhere would be a real data loss,
        //   not a cosmetic one.
        //
        // The Target arrays are emitted whenever `modifiers` is present;
        // when the caller ran the engine WITHOUT the per-target split (the
        // native `--format json` path, which has no use for it) every slot
        // is simply `[]`, which is the shape EI emits for a target with no
        // rows anyway.
        if let Some(mods) = modifiers {
            let obj = v.as_object_mut().expect("player value is always a JSON object");
            let (outgoing, incoming) = ei_damage_mod_split(
                mods.overall
                    .range((p.agent_addr, i32::MIN)..=(p.agent_addr, i32::MAX))
                    .map(|(&(_, id), s)| (id, s))
                    .collect(),
            );
            obj.insert("damageModifiers".to_string(), json!(outgoing));
            obj.insert("incomingDamageModifiers".to_string(), json!(incoming));

            let mut out_t: Vec<Value> = Vec::with_capacity(legacy.ei_targets.len());
            let mut in_t: Vec<Value> = Vec::with_capacity(legacy.ei_targets.len());
            for e in &legacy.ei_targets {
                let (o, i) = ei_damage_mod_split(
                    mods.per_target
                        .range((p.agent_addr, e.id, i32::MIN)..=(p.agent_addr, e.id, i32::MAX))
                        .map(|(&(_, _, id), s)| (id, s))
                        .collect(),
                );
                out_t.push(json!(o));
                in_t.push(json!(i));
            }
            obj.insert("damageModifiersTarget".to_string(), json!(out_t));
            obj.insert("incomingDamageModifiersTarget".to_string(), json!(in_t));
        }
        v
    });
    // MROSTER -- the `targets[]` roster.
    //
    // `report.ei_targets`, not `report.enemies`: the two are different
    // filters over the same `enc.enemies`, and only the first one is
    // GW2EI's `LogData.Logic.Targets`. `axilog_schema::Report::ei_targets`'
    // doc comment carries the full derivation from
    // `GW2EIEvtcParser/LogLogic/WvW/WvWLogic.cs`; the short version is that
    // a WvW log's targets are the NON-SQUAD, NON-FRIENDLY PLAYER agents
    // (`WvWLogic.cs:325-375`) plus one synthetic aggregate
    // (`WvWLogic.cs:307`), and NPCs/gadgets -- siege, guards, keep lords,
    // tactivators, loot bags, pets, clones -- are never targets in a WvW
    // log no matter how much damage they exchanged.
    //
    // This used to emit every enumerated enemy instead (624 rows on the
    // local reference capture, against GW2EI's own 57 for the same log).
    // That was wrong three ways, all measured:
    //
    // 1. **Wrong shape.** It was justified as "EI keeps every enumerated
    //    target regardless of interaction", which conflated EI's
    //    *interaction* filter (EI genuinely has none) with EI's *actor
    //    kind* filter (which is the whole rule in WvW). EI drops 567 of
    //    those 624 agents, and not because they were idle.
    // 2. **Wrong numbers downstream.** axibridge's damage-mitigation
    //    aggregate folds `targets[].totalDamageDist` into a per-skill
    //    average/minimum table (`packages/bridge-metrics/src/
    //    computePlayerAggregation.ts:491-509`) with no `enemyPlayer`
    //    filter of its own -- it trusts the roster. Folded over the old
    //    624-row roster, 6 of 206 skill averages and 21 of 206 minima
    //    diverged from the GW2EI reference; folded over the enemy-player
    //    subset, all 206 were exact. Curating the roster here is what
    //    makes that exactness unconditional rather than an analysis-time
    //    restriction the consumer has to know to apply. Eight further
    //    axibridge sites count `targets.filter(t => !t.isFake).length` as
    //    "enemies in the fight" and were reading 624.
    // 3. **Payload.** The nine arrays positionally joined to `targets[]`
    //    (`statsTargets`, `dpsTargets`, `targetDamage1S`,
    //    `targetPowerDamage1S`, `targetDamageDist`, `damageModifiersTarget`
    //    `incomingDamageModifiersTarget`, and the two `target*1S` variants
    //    this project does not emit) are all
    //    `players.len() * targets.len()`-shaped, so the roster is a
    //    multiplier on the whole `--timeseries` document.
    //
    // Because the roster and all nine joined arrays are built from this
    // ONE `report.ei_targets` list, curation moves them in lockstep by
    // construction -- there is no second list to keep in sync.
    //
    // The one row GW2EI has that this project does not is its synthetic
    // aggregate (57 = 1 + 56): see `isFake` immediately below.
    //
    // M11 Task 3: `isFake` -- real EI sets this `true` for its own
    // synthetic aggregate pseudo-targets (a "sum of every real target"
    // stand-in row it adds to `targets[]` for certain fight types); every
    // one of THIS project's `ei_targets` is a real, individually-tracked
    // agent (never a synthesized aggregate), so `isFake: false` is correct
    // for every row here, not a faked/guessed value. This field was
    // previously ABSENT entirely. Verified against axibridge's own source
    // (read-only reference, `src/renderer/**`): every `targets[]` consumer
    // found (`ExpandableLogCard.tsx`, `computeCommanderStats.ts`,
    // `computeFightDiffMode.ts`, `computeFightBreakdown.ts`,
    // `incrementalAggregation.ts`) reads it via `!t.isFake`/`t?.isFake`
    // (optional-chained or negated), which already treated an absent field
    // as falsy/"not fake" the same as an explicit `false` -- so this
    // specific project never hit a live miscount from the omission. It's
    // still the correct fix: axibridge's own `dpsReportTypes.ts` declares
    // `isFake: boolean` as NON-optional (every real dps.report/EI export
    // always carries it), so an absent field was already a silent
    // contract violation for any future/less-defensive consumer that reads
    // `t.isFake` directly rather than through the truthy-check pattern
    // every current call site happens to use.
    //
    // MROSTER: that "for certain fight types" is, concretely, EVERY WvW
    // log -- `WvWLogic.cs:307` unconditionally adds one synthetic NPC
    // agent (`TargetID.WorldVersusWorld`, `"Dummy PvP Agent"` in detailed
    // mode, `"Enemy Players"` otherwise) which is `targets[0]` and the
    // only `isFake: true` row. That is the entire 57-vs-56 difference
    // between GW2EI's roster on the reference capture and this one; every
    // other row now matches one-for-one in kind. It is deliberately NOT
    // synthesized here: its `dpsAll`/`totalDamageDist`/`buffs` would be a
    // re-derived sum of the 56 real rows, i.e. a row of numbers this
    // project would be inventing rather than measuring, and every
    // axibridge consumer already excludes it (`!t.isFake`, and the
    // enemy-player readers additionally require `t.enemyPlayer`, which
    // EI's dummy has as `false`). Adding it would strictly ADD a row every
    // consumer discards. Recorded as a known, intentional roster delta
    // rather than papered over.
    //
    // M15 Task 3: real EI gives its `targets[]` the SAME `combatReplayData`
    // object it gives `players[]` (verified in the local reference export:
    // 56 of its 57 targets carry a full `{start, end, iconURL, positions,
    // orientations, dead, down, dc}`), so an enemy PLAYER target gets one
    // here too when replay was requested -- `axilog_core::analysis::
    // ei_replay` tracks them alongside squad players for exactly this.
    // (Pre-MROSTER this also had to say "NPC targets get nothing, the
    // replay engine only tracks player actors"; post-curation every row in
    // `ei_targets` on a WvW log IS a player actor, so the `e.is_player`
    // guard below is now a tautology on that path -- it is kept because
    // the guard is what advances `enemy_track`, and because the
    // non-WvW/hand-built-`Report` paths can still carry NPC rows.)
    //
    // `iconURL` USED to be the one field that could not match EI here: EI
    // takes it from the enemy's own `Spec` (its `players[]`-style
    // profession), and `model::Enemy` had no profession/elite-spec field at
    // all, only a display name -- so every enemy player got
    // `icons::UNKNOWN_PROFESSION_ICON`, EI's own unrecognized-spec fallback
    // (`ParserHelper.GetProfIcon`), rather than a guess parsed out of the
    // name string.
    //
    // MENEMYPROF closed that. The agents behind these rows were resolved as
    // full `Player`s by `model::resolve` (arcdps fills their `prof`/
    // `is_elite` agent-table columns exactly as it does for squad members);
    // `wvw::apply` was simply discarding both on the hop that reclassifies
    // them into `enemies`. It now carries them, so `iconURL` resolves
    // through the same `icons::prof_icon_url` chain the player side uses.
    //
    // The reference export still cannot be joined on spec to CONFIRM this
    // per-row -- EI's `JsonNPC` has no `profession` member, so its
    // `targets[]` carries `profession: null` on all 57 rows. This crate
    // emits that key anyway as a deliberate superset; see the `profession`
    // comment on the target object below for why.
    // Bucket count for an enemy that never dealt damage (MEIGAP Task 2b):
    // read off any enemy that did, since every series in the map is built
    // to the one length `axilog_core::analysis::timeseries::ei_grid`
    // computes (GW2EI's own `InterpolatedGraph` allocation). Falling back to
    // a player's own `per_second.damage` length keeps the arrays aligned
    // even on the degenerate log where no enemy dealt any damage at all.
    let enemy_buckets = enemy_series
        .and_then(|m| m.values().next().map(|s| s.damage.len()))
        .or_else(|| legacy.players.iter().find_map(|p| p.per_second.as_ref()).map(|ps| ps.damage.len()))
        .unwrap_or(0);
    // MSTREAM: one target row, built on demand — same treatment as
    // `player_json` above, body unchanged. This builder is genuinely
    // stateful: `enemy_track` is walked forward by the enemy-PLAYER rows
    // only, so the rows MUST be produced in ascending index order exactly
    // once. `LazySeq` does precisely that (`for i in 0..len`), which is the
    // same order the old `.map(..).collect()` produced.
    let mut enemy_track = replay.map(|r| r.tracks[legacy.players.len()..].iter());
    let detected_targets = detected.clone();
    // The `targets[]` re-point. Everything identity-shaped now comes from
    // the native `entities[]` row this slot resolves to, through the one
    // ordering helper; `dpsAll` comes from that entity's `blocks.damage`
    // row. What is still read off the side channel below -- `damage1S`,
    // `totalDamageDist`, `buffs[]`, `combatReplayData` -- is not an
    // oversight: each is an unabsorbed pass with its own later task, and
    // `ei_replay` in particular is never absorbed at all (it is derived
    // inside this adapter once `EiInputs` dies).
    let join = crate::join::EiJoin::new(report);
    let target_json: Box<dyn FnMut(usize) -> Value + 'a> = Box::new(move |target_idx: usize| {
        let entity_id = report.source_order.targets().get(target_idx).copied();
        let entity = entity_id.and_then(|id| join.entity(id));
        // EI's `targets[].id` is the enemy's agent address. Native keeps
        // that on `EntityOut::agent_addr` for every role -- for an enemy
        // it IS `EnemyOut::id`, which is what `EntityIndex::by_enemy_id`
        // joins on -- so this is the same integer the legacy row carried,
        // not a re-derivation. It also stays the key for the three
        // side-channel maps read further down, which are still keyed by
        // enemy id until their own tasks move them.
        let enemy_id = entity.map_or(0, |e| e.agent_addr);
        // `enemyPlayer` was `EnemyOut::is_player`; natively that IS the
        // role, and `build_entities` assigns the two from each other.
        let is_player = entity.is_some_and(|e| e.role == axilog_schema::v1::entities::Role::EnemyPlayer);
        let profession = entity.and_then(|e| e.profession.as_deref()).unwrap_or("");
        let elite_spec = entity.and_then(|e| e.elite_spec.as_deref()).unwrap_or("");
        let team = entity.map_or("", |e| e.team.as_str());
        let damage_out = entity_id
            .and_then(|id| report.blocks.damage.as_ref()?.by_entity.get(id))
            .map_or(0, |d| d.total);
        let team_id_for = |color: &str| -> u64 {
            detected_targets.get(color).copied().unwrap_or_else(|| representative_team_id(color))
        };
        let mut t = json!({
            "id": enemy_id,
            "name": entity_id.map_or("", |id| join.display_name(id)),
            "enemyPlayer": is_player,
            // `profession` (MENEMYPROF) -- a DELIBERATE SUPERSET of EI's
            // shape, and the only field on this object that real EI does
            // not also emit. GW2EI's `JsonNPC` has no profession member at
            // all, so the reference export's `targets[]` carries
            // `profession: null` on all 57 rows and cannot be joined on
            // spec the way `players[]` can.
            //
            // Emitted anyway because the alternative is worse: a consumer
            // grouping the enemy roster by class has nothing else to key
            // on, and falls back to the `name` string -- which in WvW is
            // the player's RANK TITLE ("Mithril Scout", "Diamond Legend"),
            // so the resulting chart buckets rank tiers and reads like the
            // roster is full of NPCs. Adding a key EI omits is safe in the
            // direction that matters (every EI consumer ignores unknown
            // keys); silently mis-charting is not.
            //
            // `null` for an NPC/gadget target, matching EI's own value
            // there, so nothing that already tolerates the reference
            // export's all-null column regresses.
            "profession": Some(elite_spec)
                .filter(|s| !s.is_empty())
                .or(Some(profession))
                .filter(|s| !s.is_empty()),
            // `dpsAll` (MEIGAP2 row 5): the enemy's OUTGOING damage total,
            // GW2EI's `JsonActor.DpsAll[0]` over `GetDamageStats(log,
            // phase)` -- minion-inclusive and `iff`-filtered; see
            // `axilog_core::analysis::Metrics::enemy_damage_out`. `dps` uses
            // the same whole-fight-seconds convention and the same
            // `(int)Math.Round(Damage / phaseDuration)` rounding the
            // player-side `dpsAll[0].dps` already does. Always-on: it is
            // two scalars per target, and axibridge reads it
            // unconditionally for the WvW enemy-team split
            // (`src/renderer/ExpandableLogCard.tsx:487`,
            // `src/main/discord.ts:179`). GW2EI's many other `JsonDPS`
            // fields on this object (`condiDps`/`powerDps`/`actor*`/
            // `breakbarDamage`) are not computed per enemy and are omitted
            // rather than faked.
            "dpsAll": [ {
                "dps": (damage_out as f64 / duration_secs).round() as i64,
                "damage": damage_out,
            } ],
            "teamID": team_id_for(team), "isFake": false
        });
        // `instanceID` (MEIGAP2 row 3), the target twin of the player field
        // above -- same `JsonActor.InstanceID` source, same absent-means-
        // unknown encoding. axibridge de-duplicates enemy targets on it
        // (`ExpandableLogCard.tsx:420,477`, `src/main/discord.ts:173,452`:
        // `t?.instanceID ?? t?.instid ?? t?.id ?? rawName`).
        if let Some(instid) = entity.and_then(|e| e.instid) {
            let obj = t.as_object_mut().expect("target value is always a JSON object");
            obj.insert("instanceID".to_string(), Value::from(instid));
        }
        if is_player {
            if let Some(track) = enemy_track.as_mut().and_then(|it| it.next()) {
                // Correction to the earlier audit fix: `combatReplayData` is
                // NOT gated on the actor having any polled positions.
                // GW2EI's `JsonActorBuilder.cs:103-104` builds it
                // UNCONDITIONALLY (`if (log.CanCombatReplay) jsonActor.
                // CombatReplayData = Build(...)`, keyed only on whether
                // replay was requested at all), and
                // `SingleActorCombatReplayDescription`'s constructor assigns
                // `Positions`/`Rotations` straight from whatever the actor's
                // polled lists hold -- an empty list serializes as `[]`, it
                // does not suppress the object. Verified against the local
                // reference export: its "Dummy PvP Agent" target (an
                // `isFake` NPC with `enemyPlayer: false`, no position data
                // at all) still carries a full `combatReplayData` object
                // with `positions: []`/`orientations: []` but real
                // `start`/`end`/`iconURL`/`dead`/`down`/`dc`. (An enemy
                // player with genuinely zero polled positions here IS,
                // separately, the real bug this project fixed: GW2EI's
                // `forcePolling` -- `SingleActor.cs:415` -- only applies to
                // squad `AgentType.Player` actors, never `NonSquadPlayer`
                // enemies, so such an actor's `positions` must be `[]`, not
                // the `(int.MinValue, int.MinValue)` sentinel a squad
                // player would get. That fix lives in
                // `axilog_core::analysis::ei_replay::build_world_track`,
                // not here -- this object's presence is unconditional
                // either way.)
                t.as_object_mut().expect("target is a JSON object").insert(
                    "combatReplayData".to_string(),
                    json!({
                        "start": track.start,
                        "end": track.end,
                        // MENEMYPROF: now that `model::Enemy` carries the
                        // profession/elite spec, this resolves through the
                        // same `prof_icon_url` chain the player side uses
                        // rather than falling back to EI's own
                        // unrecognized-spec icon. An NPC/gadget target
                        // still gets the fallback, which is correct -- it
                        // has no spec to resolve.
                        "iconURL": prof_icon_url(profession, elite_spec),
                        "positions": ei_positions_json(&track.positions),
                        "orientations": ei_orientations_json(&track.orientations),
                        "dead": ei_intervals_json(&track.dead),
                        "down": ei_intervals_json(&track.down),
                        "dc": ei_intervals_json(&track.dc),
                    }),
                );
            }
        }
        // MEIGAP Task 2b/2c/2d: the three `targets[]` mirrors. Each keys off
        // its own input's PRESENCE (the "presence, not a flag" convention
        // `skill_damage`/`per_second` already use on the player side), so a
        // flagless render emits none of them and stays byte-identical.
        let obj = t.as_object_mut().expect("target is a JSON object");
        if let Some(series) = enemy_series {
            // `targets[].damage1S`/`.powerDamage1S` are this enemy's OUTGOING
            // damage, `[phase][second]`-shaped exactly like the player-side
            // `damage1S` -- built by the SHARED `JsonActorBuilder.
            // FillJsonActor` (`JsonActorBuilder.cs:72-73`) over an NPC actor
            // (`JsonNPCBuilder.cs:20` calls it first). An enemy that never
            // dealt damage gets a full-length zero series, not an absent
            // key, matching EI's always-present arrays.
            let (damage, power) = match series.get(&enemy_id) {
                Some(s) => (s.damage.clone(), s.power_damage.clone()),
                None => (vec![0u64; enemy_buckets], vec![0u64; enemy_buckets]),
            };
            obj.insert("damage1S".to_string(), json!([damage]));
            obj.insert("powerDamage1S".to_string(), json!([power]));
        }
        if let Some(dist) = enemy_dist {
            // `targets[].totalDamageDist[0]`: this enemy's OUTGOING damage
            // by skill, ACTOR-ONLY (`GetJustActorDamageEvents`) -- see
            // `skill_damage::build_enemy_dist`'s doc comment, including why
            // the contributing-hit count is emitted as `connectedHits`
            // (the key axibridge's mitigation math divides by) rather than
            // as EI's attempt-count `hits`.
            let skills: Vec<Value> = dist
                .get(&enemy_id)
                .map(|v| v.iter().map(enemy_skill_entry_ei_json).collect())
                .unwrap_or_default();
            obj.insert("totalDamageDist".to_string(), json!([skills]));
        }
        if let Some(tc) = target_conditions {
            // `targets[].buffs[]`: `{id, statesPerSource}` per condition
            // this enemy carried, source-split by squad-player character
            // name -- see `axilog_core::analysis::target_conditions`'s
            // module doc for the direction citation and the deliberate
            // conditions-only / `statesPerSource`-only narrowing.
            let buffs: Vec<Value> = tc
                .range((enemy_id, 0u32)..=(enemy_id, u32::MAX))
                .map(|(&(_, buff_id), per_source)| {
                    let states: BTreeMap<&str, Value> = per_source
                        .iter()
                        .map(|(name, tl)| (name.as_str(), ei_states_json(tl)))
                        .collect();
                    json!({ "id": buff_id, "statesPerSource": states })
                })
                .collect();
            obj.insert("buffs".to_string(), json!(buffs));
        }
        t
    });
    // `buffMap` (M3 Task 5): a subset covering only the 12 tracked boons,
    // keyed `"b<id>"` per real EI's convention (verified against
    // axibridge's `test-fixtures/boon/20260117-181030.json`,
    // `buffMap.b740` etc). Only the two fields we actually compute/know:
    // `name` and `stacking` (`true` for the two Intensity-type boons —
    // Might, Stability — `false` for the other 10 Queue/duration-type
    // ones, matching real EI's own `stacking` bool exactly for every one
    // of the 12 ids in that same fixture). Real EI's sibling fields
    // (`classification`, `icon`, `conversionBasedHealing`, `hybridHealing`,
    // `descriptions`) aren't computed here, so they're omitted rather than
    // faked.
    //
    // MEIGAP Task 2d: when `targets[].buffs` is emitted, the fourteen
    // CONDITION ids join the map on the same terms. This is not decoration:
    // axibridge resolves every target-buff id through `resolveBuffMetaById`
    // and drops the entry outright when the lookup misses
    // (`conditionsMetrics.ts:311-314`), so without these rows the whole
    // `targets[].buffs` array would be dead payload. Gated on the same
    // input so the flagless `buffMap` stays byte-identical.
    // NOT yet re-pointed onto `report.catalogs.buffs`, though it looks
    // ready: the 12 flagless boon ids match exactly, and native's
    // `stacking: "intensity"|"duration"` maps onto EI's boolean with no
    // residue. The blocker is the CONDITION half. Native's catalog is
    // referenced-scoped, and nothing references the 14 condition ids until
    // the conditions block exists (Task 12), so re-pointing now empties
    // the condition rows out of `buffMap` while `targets[].buffs` still
    // emits those ids -- which axibridge answers by DROPPING each entry
    // whose id misses `resolveBuffMetaById`. `ei_json_meigap2_target_
    // mirrors_are_gated_and_internally_consistent` catches exactly this.
    // Re-point this map in Task 12, once conditions register natively.
    let mut buff_map: BTreeMap<String, Value> = BOON_IDS.iter().map(|&(id, name, is_intensity)| {
        (ei_catalog_key('b', i64::from(id)), json!({ "name": name, "stacking": is_intensity }))
    }).collect();
    if target_conditions.is_some() {
        for (id, name, is_intensity) in
            axilog_core::analysis::target_conditions::condition_buff_map()
        {
            buff_map.insert(
                ei_catalog_key('b', i64::from(id)),
                json!({ "name": name, "stacking": is_intensity }),
            );
        }
    }
    // `skillMap` (M14, Task 3): keyed `"s<id>"` per real EI's convention
    // (verified against axibridge's `test-fixtures/boon/
    // 20260117-181030.json`, `skillMap.s45534` etc -- same `"s"`-prefix
    // pattern `buffMap`'s `"b"` prefix above already mirrors for boons).
    // ALWAYS present (not gated) -- `Report::skill_map` itself is always
    // populated (`axilog_core::analysis::Metrics::skill_map`'s doc comment:
    // "Always computed (not opt-in)"), same always-on convention as
    // `buffMap`/`targets` above. Only the fields this project actually
    // computes are emitted (`axilog_core::analysis::skill_map::
    // SkillMapEntry`'s own fields, via `axilog_schema::SkillMapEntryOut`):
    // `name` (log-table best-effort, NOT EI's richer API-backed name --
    // see that module's doc comment's honesty writeup), `isSwap`, `canCrit`.
    // Real EI's sibling fields on the same entry -- `icon` (a
    // render.guildwars2.com URL) and `autoAttack` (needs the external GW2
    // API, this project's `auto_attack` is always `None` -- see that
    // module's doc comment's "`auto_attack`: OMITTED, not guessed"
    // section) -- as well as `isInstantCast`/`isTraitProc`/
    // `isUnconditionalProc`/`isGearProc`/`isNotAccurate`/
    // `conversionBasedHealing`/`hybridHealing` (all needing the same
    // external DB) are NOT computed anywhere in this project, so they're
    // omitted rather than faked, same "don't fake absent data" convention
    // `statsTargets`/`support`/`extHealingStats` above already follow.
    // Side-channel absorption, Task 3: sourced from
    // `report.catalogs.skills`, which now carries the whole always-on
    // `Metrics::skill_map` rather than referenced ids alone -- see
    // `axilog_schema::v1::catalogs::CatalogBuilder`'s doc comment for why
    // that invariant was relaxed for skills specifically.
    let skill_map: BTreeMap<String, Value> = report.catalogs.skills.iter().map(|(&id, e)| {
        (ei_catalog_key('s', i64::from(id)), json!({
            "name": e.name,
            "isSwap": e.is_swap,
            "canCrit": e.can_crit,
        }))
    }).collect();
    // `wvWMapData` (Task 3 of the side-channel absorption): sourced from
    // `report.encounter.teams` (the 1.0 `ReportV1`), not `legacy`'s -- the
    // first of this crate's fields to actually read off `report`. Safe by
    // construction: `ReportV1::EncounterOut::teams` is a straight
    // `legacy.encounter.teams.clone()` (see `axilog_schema::v1::
    // build_report_v1`), so this is byte-for-byte the same detection the
    // shared `team_id_for` above computes, just off the other struct. A
    // deliberately separate `detected_team_ids` call (not the outer
    // `team_id_for`) because `players[]`/`targets[]` team ids stay on
    // `legacy` until Tasks 4/5 move them.
    let wvw_detected = detected_team_ids(&report.encounter.teams);
    let wvw_team_id_for = |color: &str| -> u64 {
        wvw_detected.get(color).copied().unwrap_or_else(|| representative_team_id(color))
    };
    let wvw_map_data = json!({
        "redTeamID": wvw_team_id_for("red"),
        "blueTeamID": wvw_team_id_for("blue"),
        "greenTeamID": wvw_team_id_for("green")
    });
    // `damageModMap` (M16, Task 3): `"d<signed id>"` ->
    // `{ name, icon, description, nonMultiplier, isCounter, skillBased,
    // approximate, incoming }` -- all eight fields real EI's `DamageModDesc`
    // carries (`GW2EIBuilders/JsonModels/JsonLogBuilder.cs:308-322`; the
    // `"d"` prefix is added at `:326-330`), verified as the exact field set
    // on all 75 entries of the reference WvW export.
    //
    // Unlike `skillMap`/`buffMap` above this key is OMITTED entirely rather
    // than emitted empty when `--modifiers` was not requested: an empty
    // `{}` would be a claim that no player triggered any modifier, whereas
    // the engine simply did not run. It is also what keeps the flagless
    // ei-json output byte-identical across this milestone.
    //
    // Scoped to referenced ids, matching EI, which fills its own
    // `damageModMap` lazily from inside the per-player emission loop
    // (`JsonDamageModifierDataBuilder.cs:47-51`) -- a catalogued modifier no
    // player triggered never appears.
    //
    // Side-channel absorption, Task 3: read from `report.catalogs
    // .damage_mods`. Two of EI's eight fields are not stored there
    // verbatim. `incoming` is not stored at all because the map key's SIGN
    // already encodes it -- negative ids are incoming (see
    // `axilog_schema::v1::catalogs::DamageModEntry`'s doc comment); that
    // equivalence was checked against `incoming` on all 59 referenced ids
    // of the committed fixture, zero disagreements, so deriving it here
    // reproduces the field rather than approximating it. `icon` WAS
    // missing and has been added to the native entry by this task: the
    // adapter emitting a value the native document cannot produce is
    // exactly the superset violation this milestone exists to remove.
    let damage_mod_map: Option<BTreeMap<String, Value>> = modifiers.map(|_| {
        report
            .catalogs
            .damage_mods
            .iter()
            .map(|(&id, m)| {
                (ei_catalog_key('d', i64::from(id)), json!({
                    "name": m.name,
                    "icon": m.icon,
                    "description": m.description,
                    "nonMultiplier": m.non_multiplier,
                    "isCounter": m.is_counter,
                    "skillBased": m.skill_based,
                    "approximate": m.approximate,
                    "incoming": id < 0,
                }))
            })
            .collect()
    });
    // `combatReplayMetaData` (M15, Task 3): the arena image every
    // `combatReplayData.positions` pair is a pixel coordinate ON -- present
    // only when replay was requested AND the log's map is one GW2EI ships
    // an image for (`axilog_core::analysis::ei_replay::combat_replay_meta`
    // returns `None` otherwise: Obsidian Sanctum, Armistice Bastion, and
    // any non-WvW map id). EI's own field is nullable, and OMITTING it is
    // the honest encoding of "these coordinates are on a computed bounding
    // box, not on a published image" -- the `positions` themselves are
    // still emitted in that case, via
    // `ei_replay::bounding_box_transform`, exactly as GW2EI does.
    //
    // Every float here goes through `ei_float` -- `inchToPixel` is the
    // canonical trap (`0.009` as a C# `float`; widened to `f64` its
    // shortest round-trip becomes `0.008999999612569809`).
    let combat_replay_meta = replay.and_then(|r| r.meta.as_ref()).map(|meta| {
        json!({
            "inchToPixel": ei_float(f64::from(meta.inch_to_pixel)),
            "pollingRate": meta.polling_rate,
            "sizes": meta.sizes,
            "maps": meta.maps.iter().map(|m| json!({
                "url": m.url,
                "interval": m.interval,
                "position": [ei_float(m.position[0]), ei_float(m.position[1])],
            })).collect::<Vec<_>>(),
        })
    });
    // `recordedBy` (Task 3): `report.encounter.recorded_by` is an ENTITY ID
    // (`ReportV1`'s id-first roster design -- see `EncounterOut::
    // recorded_by`'s doc comment), not the account string this field needs,
    // so it takes the one extra hop through `entities[]` rather than
    // duplicating the account string onto `encounter` as a convenience
    // (RULING T3-2: the id-first roster is the design; a second identity
    // source would undermine it).
    let recorded_by = report
        .encounter
        .recorded_by
        .and_then(|id| report.entities.get(id as usize))
        .and_then(|e| e.account.as_deref());
    EiDoc {
        fight_name: format!("Detailed WvW - {}", report.encounter.map),
        duration_ms: report.encounter.duration_ms,
        recorded_by,
        player_count: legacy.players.len(),
        player_json: LazyRows::new(player_json),
        target_count: legacy.ei_targets.len(),
        target_json: LazyRows::new(target_json),
        buff_map,
        skill_map,
        damage_mod_map,
        combat_replay_meta,
        wvw_map_data,
    }
}

/// Stream the ei-json document straight into `w` (MSTREAM).
///
/// This is the memory-critical path — the CLI's `--format ei-json` uses it,
/// with or without `-o`. Peak resident memory is the analysis results plus
/// one player row plus `w`'s own buffer, instead of the whole document tree
/// AND its pretty-printed `String` (the pre-MSTREAM CLI held both at once).
///
/// Output is byte-identical to `serde_json::to_string_pretty(&to_ei_json(..))`
/// — same `PrettyFormatter`, same key order, same numbers (every float still
/// goes through `ei_float`/`ei_double`/`ei_damage_gain`/`ei_breakbar` and is
/// carried as a `Value` into the serializer). No trailing newline is written;
/// the CLI appends its own, exactly as it did when it `format!`-ed one on.
///
/// `report`/`legacy` is the side-channel absorption's TRANSITIONAL
/// three-argument form (Task 3 of that plan): `report` is the 1.0
/// [`ReportV1`], `legacy` the pre-absorption [`Report`]. Each later task in
/// that plan drains one more surface off `legacy` onto `report`; the final
/// task deletes both this parameter split and `EiInputs`.
pub fn write_ei_json<W: std::io::Write>(
    report: &ReportV1,
    legacy: &Report,
    inputs: &EiInputs<'_>,
    w: W,
) -> serde_json::Result<()> {
    let doc = ei_doc(report, legacy, inputs);
    let mut ser = serde_json::Serializer::with_formatter(
        w,
        serde_json::ser::PrettyFormatter::new(),
    );
    serde::Serialize::serialize(&doc, &mut ser)
}

/// Render a [`Report`] plus its EI-only side inputs as an Elite-Insights-
/// compatible `serde_json::Value`.
///
/// The SDK-facing entry point: `axilog-node` hands the tree to napi and
/// `axilog-py` hands it to pythonize, both of which need a materialized
/// `Value` anyway, so there is nothing to stream away for them. It shares
/// [`ei_doc`] with [`write_ei_json`] — one definition of the format, no
/// second tree-builder to drift.
///
/// Prefer [`write_ei_json`] whenever the destination is a file, a socket, or
/// stdout: this function's peak memory is the whole document.
///
/// See [`EiInputs`] for what each input gates; `EiInputs::default()` renders
/// everything that is derivable from the `Report` alone.
pub fn to_ei_json(report: &ReportV1, legacy: &Report, inputs: &EiInputs<'_>) -> Value {
    let doc = ei_doc(report, legacy, inputs);
    serde_json::to_value(&doc).expect("ei-json document is infallibly serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cheapest valid `ReportV1` for the unit tests below, which
    /// predate the native container and hand-build a legacy `Report`
    /// directly -- often with no `Encounter`/`Metrics` in scope to run
    /// `axilog_schema::v1::build_report_v1` against. RULING T3-3 (side-
    /// channel absorption plan, Task 3): these tests stay on the legacy
    /// path and are not migrated to build a real `ReportV1` here -- they
    /// exercise rendering off `legacy`, not the four scalars this task
    /// moved onto `report` (`fightName`/`durationMS`/`recordedBy`/
    /// `wvWMapData`), so an empty shell is sufficient. They migrate for
    /// real in Task 13, when `EiInputs`/the two-`Report` split are deleted.
    fn empty_report_v1() -> axilog_schema::v1::ReportV1 {
        axilog_schema::v1::ReportV1 {
            axilog: axilog_schema::v1::envelope::AxilogMeta {
                schema: "1.0",
                version: String::new(),
                generated_from: None,
            },
            encounter: axilog_schema::v1::EncounterOut {
                kind: String::new(),
                map: String::new(),
                duration_ms: 0,
                build: String::new(),
                revision: 0,
                recorded_by: None,
                teams: vec![],
                markers: vec![],
                tick_rate: None,
            },
            entities: vec![],
            source_order: axilog_schema::v1::SourceOrder::default(),
            catalogs: axilog_schema::v1::catalogs::Catalogs::default(),
            blocks: axilog_schema::v1::Blocks::default(),
            coverage: axilog_schema::v1::envelope::Coverage::new(),
            warnings: vec![],
        }
    }

    /// The one `Encounter`/`Metrics` pair behind both `sample_report()`
    /// and `sample_report_v1()`. They used to carry a copy each, which is
    /// a real hazard now that the adapter reads BOTH documents for the
    /// same row: the moment the copies drift, a test asserts against a
    /// pair no pipeline could ever produce, and it passes.
    fn sample_inputs() -> (axilog_core::model::Encounter, axilog_core::analysis::Metrics) {
        // Construct via axilog_schema public API by round-tripping from core types.
        use axilog_core::model::{Encounter, Player};
        use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
        use axilog_core::model::Enemy;
        let enc = Encounter{kind:"wvw".into(),map:"Eternal Battlegrounds".into(),
            duration_ms:1000,build:"".into(),revision:1,recorded_by:Some(":A.1".into()),
            teams:vec![],players:vec![Player{agent_addr:1,account:":A.1".into(),
            character:"A".into(),profession:"Thief".into(),elite_spec:"Daredevil".into(),
            team:"red".into(),subgroup:2,in_squad:true,commander:true,marker:None,commander_tag:None,guild_id:None,agent_addrs:vec![1]},
            Player{agent_addr:2,account:":B.2".into(),
            character:"B".into(),profession:"Guardian".into(),elite_spec:"".into(),
            team:"red".into(),subgroup:2,in_squad:true,commander:false,marker:None,commander_tag:None,guild_id:None,agent_addrs:vec![2]}],
            enemies:vec![Enemy{id:9,instid:9,name:"Foe".into(),team:"blue".into(),
            is_player:true,marker:None,
            profession:Some("Necromancer".into()),elite_spec:Some("Reaper".into()),
            agent_addrs:vec![9]},
            Enemy{id:10,instid:10,name:"Gadget".into(),team:"blue".into(),
            is_player:false,marker:None,profession:None,elite_spec:None,
            agent_addrs:vec![10]}],
            markers:vec![],tick_rate:None};
        use axilog_core::analysis::contribution::ContributionMetrics;
        let m = Metrics{players:vec![
            PlayerMetrics{agent_addr:1,damage_total:500,dps:500.0,per_enemy:vec![(9,500)],
            downs_dealt:1,kills_dealt:1,
            downs_contribution: ContributionMetrics{damage:400,..Default::default()},deaths:0,
            cc_applied:3,cc_duration_ms:1200,..Default::default()},
            PlayerMetrics{agent_addr:2,damage_total:300,dps:300.0,
            downs_dealt:0,kills_dealt:0,deaths:1,..Default::default()}],
            timeline:Timeline{resolution_ms:1000,squad_damage:vec![800],cc_applied:vec![0],downs:vec![0]},
            boons: Default::default(), boon_uptime: Default::default(),
            boon_generation: Default::default(), warnings: Default::default(),
            has_healing_extension: Default::default(),
            // M10 Task 3: enemy 9 (player) took damage -- a real combat
            // participant; enemy 10 (gadget) never interacted, matching what
            // `analyze()` would actually compute for this exact scenario.
            // MROSTER: `to_ei_json` reads `report.ei_targets`, which drops
            // enemy 10 for being an NPC (not for being idle) -- the two
            // filters happen to agree on this fixture, but they are
            // independent; see `maps_core_ei_fields` below.
            combat_participant_enemies: [9u64].into_iter().collect(), instance_ids: Default::default(), enemy_damage_out: Default::default(), skill_map: Default::default()};
        (enc, m)
    }

    fn sample_report() -> axilog_schema::Report {
        let (enc, m) = sample_inputs();
        axilog_schema::build_report(&enc,&m,"0.1.0", None, None, false, false, false, None)
    }

    /// `sample_report()`'s `ReportV1` counterpart, built from the SAME
    /// `sample_inputs()`.
    ///
    /// The rule for choosing between this and `empty_report_v1()` moved
    /// with the `targets[]` re-point, and is now: **any test whose legacy
    /// report is `sample_report()` must pair it with this one.** The old
    /// rule ("the shell is enough unless you assert on `durationMS`/
    /// `recordedBy`") held only while the re-pointed fields were all
    /// numeric -- an empty `report` then merely zeroed them, which no
    /// assertion happened to read. It stops holding the moment a
    /// re-pointed field is STRUCTURAL: `enemyPlayer` comes from the
    /// entity's role, so an empty `report` does not zero it, it makes
    /// every target an NPC and deletes the enemy roster the test is about.
    ///
    /// `empty_report_v1()` remains correct for the hand-built-`Report`-
    /// literal tests further down, which have no `Encounter`/`Metrics` to
    /// give `build_report_v1` at all and assert only on surfaces still
    /// read from `legacy`.
    fn sample_report_v1() -> axilog_schema::v1::ReportV1 {
        let (enc, m) = sample_inputs();
        let legacy = axilog_schema::build_report(&enc,&m,"0.1.0", None, None, false, false, false, None);
        axilog_schema::v1::build_report_v1(&enc, &m, &legacy, "0.1.0", None, &Default::default())
    }

    #[test]
    fn maps_core_ei_fields() {
        let v = to_ei_json(&sample_report_v1(), &sample_report(), &EiInputs::default());
        assert_eq!(v["durationMS"], 1000);
        assert_eq!(v["recordedBy"], ":A.1");
        assert_eq!(v["players"][0]["account"], ":A.1");
        assert_eq!(v["players"][0]["character_name"], "A");
        // EI-style: elite-spec name wins over base profession when present.
        assert_eq!(v["players"][0]["profession"], "Daredevil");
        assert_eq!(v["players"][0]["elite_spec"], "Daredevil");
        // ...but falls back to the base profession for a core (no elite spec) build.
        assert_eq!(v["players"][1]["profession"], "Guardian");
        assert_eq!(v["players"][1]["elite_spec"], "");
        assert_eq!(v["players"][0]["hasCommanderTag"], true);
        assert_eq!(v["players"][0]["dpsAll"][0]["damage"], 500);
        // Whole-fight aggregates (downContribution/killed/downed/CC) live
        // under statsAll[0], matching real EI's `statsAll[phase]` shape.
        assert_eq!(v["players"][0]["statsAll"][0]["downContribution"], 400);
        assert_eq!(v["players"][0]["statsAll"][0]["killed"], 1);
        assert_eq!(v["players"][0]["statsAll"][0]["downed"], 1);
        assert_eq!(v["players"][0]["statsAll"][0]["appliedCrowdControl"], 3);
        assert_eq!(v["players"][0]["statsAll"][0]["appliedCrowdControlDuration"], 1200);
        // Player 2 dealt no CC/downs — statsAll still present, all zero, not faked.
        assert_eq!(v["players"][1]["statsAll"][0]["appliedCrowdControl"], 0);
        // MROSTER: statsTargets has one entry per CURATED target -- the one
        // enemy player here, not the gadget -- carrying the one per-target
        // metric we actually compute (damage).
        assert_eq!(v["players"][0]["statsTargets"].as_array().unwrap().len(), 1);
        assert_eq!(v["players"][0]["statsTargets"][0][0]["totalDmg"], 500);
        assert_eq!(v["players"][1]["statsTargets"][0][0]["totalDmg"], 0);
        assert_eq!(v["players"][0]["defenses"][0]["deadCount"], 0);
        assert_eq!(v["targets"][0]["id"], 9);
        // Verify enemyPlayer flag matches the actual is_player field.
        assert_eq!(v["targets"][0]["enemyPlayer"], true, "player enemy should have enemyPlayer: true");
        // MROSTER: `targets[]` is GW2EI's WvW roster -- non-squad enemy
        // PLAYERS (`WvWLogic.cs:325-375`). The gadget (enemy 10) is gone,
        // and `report.enemies` (native/HTML) is a SEPARATE filter that
        // happens to drop it too, for the different reason that it never
        // interacted (`Metrics::combat_participant_enemies`). Both
        // per-target arrays above must shrink with the roster, which is
        // guaranteed structurally: they are built from the same list.
        let report = sample_report();
        assert_eq!(report.enemies.len(), 1, "native enemies[] is filtered to the one participant");
        assert_eq!(report.ei_targets.len(), 1, "ei_targets is enemy players only");
        assert_eq!(v["targets"].as_array().unwrap().len(), 1, "ei-json targets[] is curated to enemy players");
        assert!(
            !v["targets"].as_array().unwrap().iter().any(|t| t["id"] == 10),
            "the NPC/gadget enemy must not appear in the EI targets[] roster"
        );
    }

    /// MROSTER: the positional-lockstep contract, stated as a test rather
    /// than only as a comment. EVERY array `to_ei_json` joins to
    /// `targets[]` by index must have exactly `targets.len()` slots, or a
    /// consumer doing `targets[i]` / `player.statsTargets[i]` mis-attributes
    /// one enemy's numbers to another. This asserts it over the full opt-in
    /// surface at once, so a future array added without the join is caught.
    #[test]
    fn every_target_joined_array_has_one_slot_per_target() {
        let report = sample_report();
        let v = to_ei_json(&sample_report_v1(), &report, &EiInputs::default());
        let n = v["targets"].as_array().expect("targets").len();
        assert_eq!(n, report.ei_targets.len(), "targets[] length is ei_targets' length");
        for p in v["players"].as_array().expect("players") {
            for key in [
                "statsTargets",
                "dpsTargets",
                "targetDamage1S",
                "targetPowerDamage1S",
                "targetDamageDist",
                "damageModifiersTarget",
                "incomingDamageModifiersTarget",
            ] {
                // Each of these is opt-in; when present it must be joined.
                if let Some(arr) = p.get(key).and_then(|x| x.as_array()) {
                    assert_eq!(arr.len(), n, "{key} must have exactly one slot per targets[] entry");
                }
            }
        }
    }

    /// M3 Task 5: `buffMap`, `buffUptimes[]`, and the four new `support[0]`
    /// fields — shape plus a known numeric value for each, built from
    /// explicit `boon_uptime`/`boon_generation`/`support` values (not the
    /// golden fixture, which lives in `axilog-core`'s own calibration
    /// tests) so this crate's unit test stays self-contained.
    /// Returns the 1.0 document alongside the legacy one, both built from
    /// the SAME `enc`/`metrics`. The three tests below used to pair this
    /// with `empty_report_v1()`, which was harmless only while every field
    /// they assert still read off `legacy`. Task 3 moved `buffMap` onto
    /// `report.catalogs`, so an empty `report` now means an empty
    /// `buffMap` -- a test artifact, not a regression (every real-fixture
    /// golden stayed byte-identical). Pairing the two sources here is what
    /// keeps these tests meaningful as Tasks 4-13 drain `legacy`.
    fn sample_v1_and_report_with_boons()
    -> (axilog_schema::v1::ReportV1, axilog_schema::Report) {
        let legacy = sample_report_with_boons();
        let (enc, m) = sample_boon_inputs();
        let v1 = axilog_schema::v1::build_report_v1(&enc, &m, &legacy, "0.1.0", None, &Default::default());
        (v1, legacy)
    }

    fn sample_report_with_boons() -> axilog_schema::Report {
        let (enc, m) = sample_boon_inputs();
        axilog_schema::build_report(&enc, &m, "0.1.0", None, None, false, false, false, None)
    }

    /// The shared `enc`/`metrics` pair both sample builders above project.
    fn sample_boon_inputs() -> (axilog_core::model::Encounter, axilog_core::analysis::Metrics) {
        use axilog_core::model::{Encounter, Player};
        use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
        use axilog_core::analysis::buffs::{self, BoonUptime, GenerationStats};
        use axilog_core::analysis::support::SupportMetrics;
        let enc = Encounter{kind:"wvw".into(),map:"Eternal Battlegrounds".into(),
            duration_ms:1000,build:"".into(),revision:1,recorded_by:None,
            teams:vec![],players:vec![Player{agent_addr:1,account:":A.1".into(),
            character:"Nim Iss".into(),profession:"Thief".into(),elite_spec:"".into(),
            team:"red".into(),subgroup:1,in_squad:true,commander:false,marker:None,commander_tag:None,guild_id:None,agent_addrs:vec![1]}],
            enemies:vec![],markers:vec![],tick_rate:None};
        let mut boon_uptime = std::collections::BTreeMap::new();
        // Might (intensity): avg_stacks=3.5, presence_pct=100.0.
        boon_uptime.insert((1u64, buffs::MIGHT), BoonUptime { presence_pct: 100.0, avg_stacks: 3.5 });
        // Quickness (duration): presence_pct=42.0 (avg_stacks meaningless/0).
        boon_uptime.insert((1u64, buffs::QUICKNESS), BoonUptime { presence_pct: 42.0, avg_stacks: 0.0 });
        let mut boon_generation = std::collections::BTreeMap::new();
        boon_generation.insert((1u64, buffs::MIGHT), GenerationStats { self_pct: 1.5, group_pct: 2.0, squad_pct: 3.0, self_wasted: 0.5, group_wasted: 0.25, squad_wasted: 0.125 });
        let m = Metrics{ instance_ids: Default::default(), enemy_damage_out: Default::default(),
            players: vec![PlayerMetrics{agent_addr:1,
                support: SupportMetrics { cleanses: 5, cleanses_self: 2, strips: 7, strips_duration_ms: 12345, resurrects: 1 },
                ..Default::default()}],
            timeline: Timeline{resolution_ms:1000,squad_damage:vec![0],cc_applied:vec![0],downs:vec![0]},
            boons: Default::default(), boon_uptime, boon_generation,
            warnings: Default::default(),
            has_healing_extension: Default::default(),
            combat_participant_enemies: Default::default(),
            skill_map: Default::default(),
        };
        (enc, m)
    }

    #[test]
    fn buff_map_covers_the_12_tracked_boons_with_computed_fields_only() {
        let (v1, legacy) = sample_v1_and_report_with_boons();
        let v = to_ei_json(&v1, &legacy, &EiInputs::default());
        let buff_map = v["buffMap"].as_object().expect("buffMap must be an object");
        assert_eq!(buff_map.len(), 12, "exactly the 12 tracked boons");
        // Known value: Might (740) is Intensity-type -> stacking: true.
        assert_eq!(v["buffMap"]["b740"]["name"], "Might");
        assert_eq!(v["buffMap"]["b740"]["stacking"], true);
        // Known value: Quickness (1187) is duration-type -> stacking: false.
        assert_eq!(v["buffMap"]["b1187"]["name"], "Quickness");
        assert_eq!(v["buffMap"]["b1187"]["stacking"], false);
    }

    #[test]
    fn buff_uptimes_map_intensity_and_duration_boons_to_ei_field_meanings() {
        let (v1, legacy) = sample_v1_and_report_with_boons();
        let v = to_ei_json(&v1, &legacy, &EiInputs::default());
        let entries = v["players"][0]["buffUptimes"].as_array().expect("buffUptimes must be an array");
        assert_eq!(entries.len(), 12, "one entry per tracked boon");
        let might = entries.iter().find(|e| e["id"] == 740).expect("Might entry present");
        // Intensity boon: EI's `uptime` is our raw avg_stacks (no *100),
        // `presence` is our presence_pct.
        assert_eq!(might["buffData"][0]["uptime"], 3.5);
        assert_eq!(might["buffData"][0]["presence"], 100.0);
        // `generated` only carries the one character-attributable entry we
        // actually compute: the player's own self-generation.
        assert_eq!(might["buffData"][0]["generated"]["Nim Iss"], 1.5);
        let quickness = entries.iter().find(|e| e["id"] == 1187).expect("Quickness entry present");
        // Duration boon: EI's `uptime` is our presence_pct, `presence` is
        // always 0 (EI never populates it for this branch).
        assert_eq!(quickness["buffData"][0]["uptime"], 42.0);
        assert_eq!(quickness["buffData"][0]["presence"], 0.0);
    }

    #[test]
    fn support_block_carries_the_four_new_computed_fields() {
        let (v1, legacy) = sample_v1_and_report_with_boons();
        let v = to_ei_json(&v1, &legacy, &EiInputs::default());
        let support = &v["players"][0]["support"][0];
        assert_eq!(support["condiCleanse"], 5);
        assert_eq!(support["condiCleanseSelf"], 2);
        assert_eq!(support["boonStrips"], 7);
        assert_eq!(support["resurrects"], 1);
        // stunBreak fields (already present since M1) are untouched.
        assert_eq!(support["stunBreak"], 0);
    }

    /// M10 Task 1: `extHealingStats`/`extBarrierStats` are present only for
    /// a player whose native `healing` block is `Some` (i.e. the log
    /// carries the healing extension), using EI's exact real field names
    /// for the subset this project actually computes, and absent entirely
    /// otherwise (native `healing: None` -- no extension data at all).
    #[test]
    fn heals_and_barrier_map_to_ei_field_names_only_when_present() {
        use axilog_schema::{
            CcOut, ContributionOut, DamageOut, DefensesOut, EncounterOut, HealingOut, HitStatsOut,
            PlayerOut, SupportOut, TimelineOut, PerSecondOut, Report,
        };
        fn base_player(account: &str, healing: Option<HealingOut>) -> PlayerOut {
            PlayerOut {
                account: account.to_string(), character: account.to_string(),
                profession: "Guardian".into(), elite_spec: "".into(), team: "red".into(),
                subgroup: 1, in_squad: true, commander: false, marker: None, commander_tag: None, guild_id: None,
                damage: DamageOut { total: 0, dps: 0.0, per_enemy: vec![] },
                downs_dealt: 0, kills_dealt: 0, downs_taken: 0, deaths: 0,
                damage_taken: 0,
                cc: CcOut { applied_total: 0, applied_duration_ms: 0, stun_breaks: 0, removed_stun_duration_ms: 0 },
                downs_contribution: ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 },
                per_target: None,
                downed_by: ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 },
                boons: vec![],
                support: SupportOut { cleanses: 0, cleanses_self: 0, strips: 0, strips_duration_ms: 0, resurrects: 0 },
                healing,
                skill_damage: None,
                per_second: None,
                dps_targets: vec![],
                hit_stats: HitStatsOut::default(),
                aftercast: Default::default(),
                defenses: DefensesOut::default(),
                rotation: None,
                damage_mods: None,
                agent_addr: 0, instid: None, breakbar_damage_dealt: 0,
                downs_contribution_per_skill: Default::default(),
            }
        }
        let report = Report {
            schema_version: "0.2", axilog_version: "0.1.0".to_string(),
            encounter: EncounterOut { kind: "wvw".into(), map: "".into(), duration_ms: 10_000,
                build: "".into(), revision: 1, recorded_by: None, teams: vec![], markers: vec![], tick_rate: None },
            players: vec![
                base_player(":A.1", Some(HealingOut {
                    healing_out_total: 5000, healing_out_allies: 3000, healing_out_self: 2000,
                    barrier_out: 1000, downed_healing_out: 500,
                })),
                base_player(":B.1", None),
            ],
            enemies: vec![],
            ei_targets: vec![],
            timeline: TimelineOut { resolution_ms: 1000, per_second: PerSecondOut { squad_damage: vec![], cc_applied: vec![], downs: vec![] } },
            warnings: vec![],
            replay: None,
            missiles: None,
            skill_map: Default::default(),
            damage_mod_map: None,
        };
        let v = to_ei_json(&empty_report_v1(), &report, &EiInputs::default());
        let healing = &v["players"][0]["extHealingStats"]["outgoingHealing"][0];
        assert_eq!(healing["healing"], 5000);
        assert_eq!(healing["downedHealing"], 500);
        assert_eq!(healing["hps"], 500); // 5000 / 10s
        assert_eq!(healing["downedHps"], 50); // 500 / 10s
        let barrier = &v["players"][0]["extBarrierStats"]["outgoingBarrier"][0];
        assert_eq!(barrier["barrier"], 1000);
        assert_eq!(barrier["bps"], 100); // 1000 / 10s
        // No native-only self/allies split leaked into the EI-shaped output
        // under invented field names.
        assert!(healing.get("healingSelf").is_none());
        assert!(healing.get("healingAllies").is_none());

        assert!(
            v["players"][1].get("extHealingStats").is_none(),
            "extHealingStats must be absent for a player with no native healing block"
        );
        assert!(v["players"][1].get("extBarrierStats").is_none());
    }

    /// M11 Task 3: `isFake` -- every target gets `false` (this project
    /// never emits real EI's synthetic aggregate pseudo-targets); the
    /// golden-fixture calibration test (`crates/axilog-ei/tests/
    /// ei_golden.rs`) asserts the same against a real multi-target log.
    #[test]
    fn every_target_is_marked_not_fake() {
        let v = to_ei_json(&sample_report_v1(), &sample_report(), &EiInputs::default());
        let targets = v["targets"].as_array().expect("targets must be an array");
        assert_eq!(targets.len(), 1, "sample_report has 1 enemy PLAYER (see ei_targets above)");
        for t in targets {
            assert_eq!(t["isFake"], false, "every real (non-aggregate) target must be isFake: false");
        }
    }

    /// M11 Task 3: `activeTimes`/`combatReplayData` are ALWAYS present (not
    /// gated on `--replay`), with harmless zero/empty defaults when the
    /// caller passes no `activity` data at all (`&[]`).
    #[test]
    fn active_times_and_combat_replay_data_default_to_zero_when_no_activity_supplied() {
        let v = to_ei_json(&sample_report_v1(), &sample_report(), &EiInputs::default());
        assert_eq!(v["players"][0]["activeTimes"], json!([0]));
        assert_eq!(v["players"][0]["combatReplayData"]["start"], 0);
        assert_eq!(v["players"][0]["combatReplayData"]["end"], 0);
        assert_eq!(v["players"][0]["combatReplayData"]["down"], json!([]));
        assert_eq!(v["players"][0]["combatReplayData"]["dead"], json!([]));
    }

    /// M11 Task 3: real (non-empty) `activity` data flows through to
    /// `activeTimes`/`combatReplayData`, positionally joined to
    /// `report.players` by index -- `sample_report()` has 2 players (agent
    /// addrs 1 and 2, in that order), matching `activity`'s own order here.
    #[test]
    fn active_times_and_combat_replay_data_map_real_activity_intervals() {
        use axilog_core::analysis::replay::Interval;
        let activity = vec![
            ActivityIntervals {
                agent_addr: 1,
                start_ms: 100,
                end_ms: 10_100,
                down_intervals: vec![Interval { start_ms: 2_000, end_ms: 3_000 }],
                dead_intervals: vec![Interval { start_ms: 5_000, end_ms: 5_500 }],
            },
            ActivityIntervals {
                agent_addr: 2, start_ms: 0, end_ms: 1_000,
                down_intervals: vec![], dead_intervals: vec![],
            },
        ];
        let v = to_ei_json(&sample_report_v1(), &sample_report(), &EiInputs { activity: &activity, ..Default::default() });
        assert_eq!(v["players"][0]["combatReplayData"]["start"], 100);
        assert_eq!(v["players"][0]["combatReplayData"]["end"], 10_100);
        assert_eq!(v["players"][0]["combatReplayData"]["down"], json!([[2_000, 3_000]]));
        assert_eq!(v["players"][0]["combatReplayData"]["dead"], json!([[5_000, 5_500]]));
        // active_ms = (10100-100) - 500 dead = 9500; down NOT subtracted.
        assert_eq!(v["players"][0]["activeTimes"], json!([9_500]));
        assert_eq!(v["players"][1]["combatReplayData"]["down"], json!([]));
        assert_eq!(v["players"][1]["activeTimes"], json!([1_000]));
    }

    /// Shared player-row builder for the M12 Task 3 tests below -- same
    /// "hand-build a `PlayerOut`" pattern `heals_and_barrier_map_to_ei_
    /// field_names_only_when_present`'s `base_player` already uses, extended
    /// with `skill_damage`/`per_second`/`dps_targets` parameters so each
    /// test only has to specify what it cares about.
    fn skill_and_timeseries_player(
        skill_damage: Option<axilog_schema::SkillDamageOut>,
        per_second: Option<axilog_schema::PlayerPerSecondOut>,
        dps_targets: Vec<axilog_schema::DpsTargetOut>,
    ) -> axilog_schema::PlayerOut {
        use axilog_schema::{CcOut, ContributionOut, DamageOut, DefensesOut, HitStatsOut, PlayerOut, SupportOut};
        PlayerOut {
            account: ":A.1".into(), character: "A".into(), profession: "Guardian".into(),
            elite_spec: "".into(), team: "red".into(), subgroup: 1, in_squad: true,
            commander: false, marker: None, commander_tag: None, guild_id: None,
            damage: DamageOut { total: 0, dps: 0.0, per_enemy: vec![] },
            downs_dealt: 0, kills_dealt: 0, downs_taken: 0, deaths: 0, damage_taken: 0,
            cc: CcOut { applied_total: 0, applied_duration_ms: 0, stun_breaks: 0, removed_stun_duration_ms: 0 },
            downs_contribution: ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 },
            per_target: None,
            downed_by: ContributionOut { damage: 0, cc: 0, strips: 0, movement_impairing: 0 },
            boons: vec![],
            support: SupportOut { cleanses: 0, cleanses_self: 0, strips: 0, strips_duration_ms: 0, resurrects: 0 },
            healing: None,
            skill_damage, per_second, dps_targets,
            hit_stats: HitStatsOut::default(),
            aftercast: Default::default(),
            defenses: DefensesOut::default(),
            rotation: None,
            damage_mods: None,
            agent_addr: 0, instid: None, breakbar_damage_dealt: 0,
                downs_contribution_per_skill: Default::default(),
        }
    }

    fn report_with_players(
        ei_targets: Vec<axilog_schema::EnemyOut>,
        players: Vec<axilog_schema::PlayerOut>,
    ) -> axilog_schema::Report {
        use axilog_schema::{EncounterOut, PerSecondOut, Report, TimelineOut};
        Report {
            schema_version: "0.2", axilog_version: "0.1.0".to_string(),
            encounter: EncounterOut { kind: "wvw".into(), map: "".into(), duration_ms: 2_000,
                build: "".into(), revision: 1, recorded_by: None, teams: vec![], markers: vec![], tick_rate: None },
            players,
            enemies: vec![],
            ei_targets,
            timeline: TimelineOut { resolution_ms: 1000, per_second: PerSecondOut { squad_damage: vec![], cc_applied: vec![], downs: vec![] } },
            warnings: vec![],
            replay: None,
            missiles: None,
            skill_map: Default::default(),
            damage_mod_map: None,
        }
    }

    /// M12 Task 3: `totalDamageDist`/`totalDamageTaken`/`targetDamageDist`
    /// are present, correctly shaped (`[phase][skillEntry]` /
    /// `[targetIndex][phase][skillEntry]`), and carry the exact computed
    /// values only when `skill_damage` was requested (`PlayerOut::
    /// skill_damage: Some(..)`).
    #[test]
    fn total_damage_dist_shape_and_known_value_when_skill_damage_present() {
        use axilog_schema::{EnemyOut, PerTargetSkillsOut, SkillDamageOut, SkillEntryOut};
        fn entry() -> SkillEntryOut {
            SkillEntryOut { skill_id: 42009, total: 32503, hits: 5, min: 100, max: 20000, crit_hits: 2, flank_hits: 1 }
        }
        let taken_entry = SkillEntryOut { skill_id: 700, total: 275, hits: 2, min: 75, max: 200, crit_hits: 0, flank_hits: 0 };
        let sd = SkillDamageOut {
            outgoing: vec![entry()],
            taken: vec![taken_entry],
            per_target: vec![PerTargetSkillsOut { enemy_id: 9, skills: vec![entry()] }],
        };
        let enemies = vec![
            EnemyOut { id: 9, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None, profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()), instid: None, damage_out: 0 },
            EnemyOut { id: 10, name: "Untouched".into(), team: "blue".into(), is_player: false, marker: None, profession: None, elite_spec: None, instid: None, damage_out: 0 },
        ];
        let player = skill_and_timeseries_player(Some(sd), None, vec![]);
        let report = report_with_players(enemies, vec![player]);

        let v = to_ei_json(&empty_report_v1(), &report, &EiInputs::default());
        let p = &v["players"][0];

        // totalDamageDist: [phase][skillEntry], only the computed fields.
        assert_eq!(p["totalDamageDist"][0][0]["id"], 42009);
        assert_eq!(p["totalDamageDist"][0][0]["totalDamage"], 32503);
        assert_eq!(p["totalDamageDist"][0][0]["min"], 100);
        assert_eq!(p["totalDamageDist"][0][0]["max"], 20000);
        assert_eq!(p["totalDamageDist"][0][0]["hits"], 5);
        assert_eq!(p["totalDamageDist"][0][0]["crit"], 2);
        assert_eq!(p["totalDamageDist"][0][0]["flank"], 1);
        assert!(p["totalDamageDist"][0][0].get("connectedDamage").is_none(), "uncomputed fields must not be faked");
        assert!(p["totalDamageDist"][0][0].get("indirectDamage").is_none());

        // totalDamageTaken: same [phase][skillEntry] shape.
        assert_eq!(p["totalDamageTaken"][0][0]["id"], 700);
        assert_eq!(p["totalDamageTaken"][0][0]["totalDamage"], 275);

        // targetDamageDist: [targetIndex][phase][skillEntry], positionally
        // keyed to `ei_targets` (enemy 9 first, enemy 10 second) -- enemy
        // 10 was never damaged, so its skill list is empty, not absent.
        assert_eq!(p["targetDamageDist"].as_array().unwrap().len(), 2, "one entry per real target");
        assert_eq!(p["targetDamageDist"][0][0][0]["id"], 42009);
        assert_eq!(p["targetDamageDist"][0][0][0]["totalDamage"], 32503);
        assert_eq!(p["targetDamageDist"][1][0], json!([]), "untouched target gets an empty skill list");
    }

    /// M12 Task 3: `totalDamageDist`/`targetDamageDist`/`totalDamageTaken`
    /// must be OMITTED entirely (not emitted empty) when the `Report` was
    /// built without `--skill-damage` (`PlayerOut::skill_damage: None`) --
    /// the gate-respecting requirement keyed off presence, not a flag.
    #[test]
    fn total_damage_dist_omitted_when_skill_damage_absent() {
        use axilog_schema::EnemyOut;
        let enemies = vec![EnemyOut { id: 9, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None, profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()), instid: None, damage_out: 0 }];
        let player = skill_and_timeseries_player(None, None, vec![]);
        let report = report_with_players(enemies, vec![player]);

        let v = to_ei_json(&empty_report_v1(), &report, &EiInputs::default());
        let p = &v["players"][0];
        assert!(p.get("totalDamageDist").is_none(), "totalDamageDist must be omitted, not emitted empty");
        assert!(p.get("totalDamageTaken").is_none());
        assert!(p.get("targetDamageDist").is_none());
    }

    /// M12 Task 3: `damage1S`/`damageTaken1S`/`targetDamage1S`/`dpsTargets`
    /// are present, correctly shaped, and the final cumulative element of
    /// `damage1S`/`damageTaken1S` matches the whole-fight scalar -- only
    /// when `per_second` was requested (`PlayerOut::per_second: Some(..)`).
    #[test]
    fn per_second_ei_fields_shape_and_known_value_when_present() {
        use axilog_schema::{DpsTargetOut, EnemyOut, PlayerPerSecondOut, PlayerTargetSeriesOut};
        let ps = PlayerPerSecondOut {
            damage: vec![50, 80, 80],
            damage_taken: vec![10, 10, 10],
            power_damage_taken: vec![6, 6, 6],
            per_target: vec![PlayerTargetSeriesOut {
                enemy_id: 9,
                damage: vec![50, 80, 80],
                power_damage: vec![50, 70, 70],
            }],
        };
        let dps_targets = vec![DpsTargetOut { enemy_id: 9, damage: 80, dps: 40.0 }];
        let enemies = vec![
            EnemyOut { id: 9, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None, profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()), instid: None, damage_out: 0 },
            EnemyOut { id: 10, name: "Untouched".into(), team: "blue".into(), is_player: false, marker: None, profession: None, elite_spec: None, instid: None, damage_out: 0 },
        ];
        let player = skill_and_timeseries_player(None, Some(ps), dps_targets);
        let report = report_with_players(enemies, vec![player]);

        let v = to_ei_json(&empty_report_v1(), &report, &EiInputs::default());
        let p = &v["players"][0];

        // damage1S/damageTaken1S: [phase][second], cumulative, final ==
        // the whole-fight total (already cumulative by construction).
        assert_eq!(p["damage1S"], json!([[50, 80, 80]]));
        assert_eq!(p["damage1S"][0].as_array().unwrap().last().unwrap(), &json!(80));
        assert_eq!(p["damageTaken1S"], json!([[10, 10, 10]]));
        // MEIGAP Task 2a: the POWER halves ride the same `per_second`
        // presence gate, and are their own series.
        assert_eq!(p["powerDamageTaken1S"], json!([[6, 6, 6]]));

        // targetDamage1S: [targetIndex][phase][second] -- enemy 9 gets the
        // real series, enemy 10 (untouched) gets an all-zero series of the
        // SAME length.
        assert_eq!(p["targetDamage1S"].as_array().unwrap().len(), 2);
        assert_eq!(p["targetDamage1S"][0], json!([[50, 80, 80]]));
        assert_eq!(p["targetDamage1S"][1], json!([[0, 0, 0]]), "untouched target gets an all-zero series, not absent");
        assert_eq!(p["targetPowerDamage1S"].as_array().unwrap().len(), 2);
        assert_eq!(p["targetPowerDamage1S"][0], json!([[50, 70, 70]]));
        assert_eq!(p["targetPowerDamage1S"][1], json!([[0, 0, 0]]));

        // dpsTargets: [targetIndex][phase]{dps, damage} -- enemy 9 carries
        // the real dps/damage, enemy 10 defaults to zero.
        assert_eq!(p["dpsTargets"][0][0]["dps"], 40);
        assert_eq!(p["dpsTargets"][0][0]["damage"], 80);
        assert_eq!(p["dpsTargets"][1][0]["dps"], 0);
        assert_eq!(p["dpsTargets"][1][0]["damage"], 0);
    }

    /// M12 Task 3: `damage1S`/`damageTaken1S`/`targetDamage1S`/`dpsTargets`
    /// must ALL be OMITTED (not emitted empty) when the `Report` was built
    /// without `--timeseries` (`PlayerOut::per_second: None`) -- including
    /// `dpsTargets`, which is gated by `per_second`'s presence rather than
    /// its own (possibly legitimately empty) `p.dps_targets` vec.
    #[test]
    fn per_second_ei_fields_omitted_when_absent() {
        use axilog_schema::EnemyOut;
        let enemies = vec![EnemyOut { id: 9, name: "Foe".into(), team: "blue".into(), is_player: true, marker: None, profession: Some("Necromancer".into()), elite_spec: Some("Reaper".into()), instid: None, damage_out: 0 }];
        let player = skill_and_timeseries_player(None, None, vec![]);
        let report = report_with_players(enemies, vec![player]);

        let v = to_ei_json(&empty_report_v1(), &report, &EiInputs::default());
        let p = &v["players"][0];
        assert!(p.get("damage1S").is_none(), "damage1S must be omitted, not emitted empty");
        assert!(p.get("damageTaken1S").is_none());
        assert!(p.get("targetDamage1S").is_none());
        assert!(p.get("dpsTargets").is_none(), "dpsTargets omitted even though the Vec field itself is always present/empty on PlayerOut");
    }

    /// M14 Task 3: `rotation[]` is present, correctly shaped (a flat array
    /// of `{ id, skills: [ { castTime, duration, timeGained, quickness } ]
    /// }`, NOT phase-wrapped -- see `to_ei_json`'s own `rotation` mapping
    /// comment for the citation against a real dps.report export), and
    /// carries the exact computed values only when `rotation` was requested
    /// (`PlayerOut::rotation: Some(..)`).
    #[test]
    fn rotation_ei_json_shape_and_known_value_when_present() {
        use axilog_schema::{CastOut, SkillRotationOut};
        let mut player = skill_and_timeseries_player(None, None, vec![]);
        player.rotation = Some(vec![SkillRotationOut {
            skill_id: 5008,
            casts: vec![
                CastOut { cast_time_ms: -100, duration_ms: 500, time_gained_ms: 20, quickness: 0.15 },
                CastOut { cast_time_ms: 600, duration_ms: 250, time_gained_ms: -50, quickness: -0.3 },
            ],
        }]);
        let report = report_with_players(vec![], vec![player]);

        let v = to_ei_json(&empty_report_v1(), &report, &EiInputs::default());
        let rotation = &v["players"][0]["rotation"];
        assert_eq!(rotation.as_array().unwrap().len(), 1, "one entry per cast skill id, no phase wrapper");
        assert_eq!(rotation[0]["id"], 5008);
        let skills = rotation[0]["skills"].as_array().unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0]["castTime"], -100);
        assert_eq!(skills[0]["duration"], 500);
        assert_eq!(skills[0]["timeGained"], 20);
        assert_eq!(skills[0]["quickness"], 0.15);
        assert_eq!(skills[1]["castTime"], 600);
        assert_eq!(skills[1]["quickness"], -0.3);
    }

    /// M14 Task 3: `rotation` must be OMITTED entirely (not emitted empty)
    /// when the `Report` was built without `--rotation` (`PlayerOut::
    /// rotation: None`) -- the gate-respecting requirement keyed off
    /// presence, not a flag, same convention `skill_damage`/`per_second`
    /// above already establish.
    #[test]
    fn rotation_ei_json_omitted_when_absent() {
        let player = skill_and_timeseries_player(None, None, vec![]); // rotation: None inside the builder
        let report = report_with_players(vec![], vec![player]);

        let v = to_ei_json(&empty_report_v1(), &report, &EiInputs::default());
        assert!(v["players"][0].get("rotation").is_none(), "rotation must be omitted, not emitted empty");
    }

    /// M14 Task 3: top-level `skillMap` is ALWAYS present (unlike
    /// `rotation`/`skill_damage`/`per_second` above), keyed `"s<id>"`, and
    /// carries only the fields this project actually computes (`name`/
    /// `isSwap`/`canCrit`) -- real EI's sibling `icon`/`autoAttack` fields
    /// (and every proc/instant/accuracy classifier) must NOT be faked in.
    #[test]
    fn skill_map_ei_json_keyed_and_carries_only_computed_fields() {
        use axilog_schema::SkillMapEntryOut;
        let player = skill_and_timeseries_player(None, None, vec![]);
        let mut report = report_with_players(vec![], vec![player]);
        report.skill_map.insert(5492, SkillMapEntryOut {
            name: "Fire Attunement".into(),
            auto_attack: None,
            is_swap: true,
            can_crit: false,
        });
        report.skill_map.insert(5008, SkillMapEntryOut {
            name: "Skill 5008".into(),
            auto_attack: None,
            is_swap: false,
            can_crit: true,
        });

        // Task 3 moved `skillMap` onto `report.catalogs.skills`, so the
        // two entries have to be staged there rather than on `legacy`.
        // Set directly instead of round-tripping a `Metrics` through
        // `build_report_v1`: this test is about the adapter's key spelling
        // and field omission, and a synthetic catalog keeps it that way.
        let mut v1 = empty_report_v1();
        for (id, name, is_swap, can_crit) in
            [(5492u32, "Fire Attunement", true, false), (5008, "Skill 5008", false, true)]
        {
            v1.catalogs.skills.insert(id, axilog_schema::v1::catalogs::SkillEntry {
                name: name.into(),
                icon: None,
                is_swap,
                can_crit,
                auto_attack: None,
            });
        }

        let v = to_ei_json(&v1, &report, &EiInputs::default());
        let skill_map = v["skillMap"].as_object().expect("skillMap must be an object");
        assert_eq!(skill_map.len(), 2);
        assert_eq!(v["skillMap"]["s5492"]["name"], "Fire Attunement");
        assert_eq!(v["skillMap"]["s5492"]["isSwap"], true);
        assert_eq!(v["skillMap"]["s5492"]["canCrit"], false);
        assert_eq!(v["skillMap"]["s5008"]["name"], "Skill 5008");
        assert_eq!(v["skillMap"]["s5008"]["isSwap"], false);
        assert_eq!(v["skillMap"]["s5008"]["canCrit"], true);
        // Real EI's `icon`/`autoAttack` (and every proc/instant/accuracy
        // classifier) are never computed by this project -- must be
        // omitted, not faked as `null`/`false`.
        assert!(v["skillMap"]["s5492"].get("icon").is_none());
        assert!(v["skillMap"]["s5492"].get("autoAttack").is_none());
    }

    /// M14 Task 3: `skillMap` is present (as an empty object, not omitted)
    /// even when `Report::skill_map` is empty -- always-on convention,
    /// matching `buffMap`'s own unconditional presence above.
    #[test]
    fn skill_map_ei_json_present_and_empty_when_report_skill_map_empty() {
        let v = to_ei_json(&sample_report_v1(), &sample_report(), &EiInputs::default());
        assert!(v.get("skillMap").is_some(), "skillMap key must always be present");
        assert_eq!(v["skillMap"].as_object().unwrap().len(), 0, "sample_report's Metrics::skill_map is Default::default() (empty)");
    }

    /// M15 Task 3: the `f32`-text contract. Each of these is a value GW2EI
    /// really emits (`inchToPixel`, a real position component, a real
    /// orientation, an integral coordinate); the assertion is on the JSON
    /// TEXT, which is the whole point -- the naive `json!(v)` on the
    /// widened `f64` produces the long form shown in the third column.
    #[test]
    fn ei_float_emits_gw2ei_float_text_not_widened_f64_text() {
        // (f32 value, EI's text)
        let cases: &[(f32, &str)] = &[
            (0.009, "0.009"),
            (246.672, "246.672"),
            (-75.179, "-75.179"),
            (295.81, "295.81"),
            (0.0, "0"),
            (247.0, "247"),
            (-3.148, "-3.148"),
        ];
        for &(v, want) in cases {
            let widened = f64::from(v);
            assert_eq!(
                serde_json::to_string(&ei_float(widened)).unwrap(),
                want,
                "ei_float({v}) text"
            );
        }
        // The trap itself, spelled out: the same values through `json!`.
        assert_eq!(
            serde_json::to_string(&json!(f64::from(0.009f32))).unwrap(),
            "0.008999999612569809",
            "this is what the naive path emits -- the reason ei_float exists"
        );
        assert_eq!(
            serde_json::to_string(&json!(f64::from(246.672f32))).unwrap(),
            "246.6719970703125"
        );
        // Non-finite never reaches this in practice (the replay engine
        // asserts finiteness); it must not panic if it ever did.
        assert_eq!(ei_float(f64::NAN), Value::Null);
        assert_eq!(ei_float(f64::INFINITY), Value::Null);
    }

    /// M15 Task 3: the replay surface is OMITTED entirely (not emitted
    /// empty) when replay was not requested -- the gate-respecting
    /// requirement, keyed off the `replay` argument's `Option` presence.
    #[test]
    fn combat_replay_surface_omitted_when_replay_absent() {
        let v = to_ei_json(&sample_report_v1(), &sample_report(), &EiInputs::default());
        assert!(v.get("combatReplayMetaData").is_none());
        let crd = &v["players"][0]["combatReplayData"];
        for k in ["positions", "orientations", "dc", "iconURL"] {
            assert!(crd.get(k).is_none(), "combatReplayData.{k} must be omitted");
        }
        // ... while M11's always-on four stay put.
        for k in ["start", "end", "down", "dead"] {
            assert!(crd.get(k).is_some(), "combatReplayData.{k} must stay always-on");
        }
        assert!(v["targets"][0].get("combatReplayData").is_none());
    }

    /// M15 Task 3: `combatReplayMetaData` is omitted -- while
    /// `combatReplayData` is still emitted -- when the log's map has no
    /// GW2EI arena image (unknown/imageless map id, so
    /// `ei_replay::combat_replay_meta` returned `None` and the coordinates
    /// came from the computed bounding box instead).
    #[test]
    fn combat_replay_meta_omitted_but_positions_kept_on_an_unmapped_log() {
        use axilog_core::analysis::ei_replay::{EiReplay, EiTrack};
        let report = sample_report();
        let track = |name: &str, is_squad: bool| EiTrack {
            agent_addr: 1,
            name: name.to_string(),
            is_squad,
            start: 0,
            end: 600,
            positions: vec![[1.5, 2.5], [3.0, 4.0]],
            orientations: vec![90.0, -0.5],
            dc: vec![[i64::MIN, 0], [600, i64::MAX]],
            down: vec![],
            dead: vec![],
        };
        // `sample_report()` has 2 players and 1 enemy PLAYER (id 9); the
        // gadget (id 10) is not in the curated EI roster at all.
        let replay = EiReplay {
            tracks: vec![track("A", true), track("B", true), track("E", false)],
            map_id: Some(899), // Obsidian Sanctum: named by GW2EI, no image
            meta: None,
        };
        let v = to_ei_json(&sample_report_v1(), &report, &EiInputs { replay: Some(&replay), ..Default::default() });
        assert!(
            v.get("combatReplayMetaData").is_none(),
            "no arena image => no metadata, even with replay on"
        );
        let crd = &v["players"][0]["combatReplayData"];
        assert_eq!(serde_json::to_string(&crd["positions"]).unwrap(), "[[1.5,2.5],[3,4]]");
        assert_eq!(serde_json::to_string(&crd["orientations"]).unwrap(), "[90,-0.5]");
        assert_eq!(
            crd["dc"],
            json!([[i64::MIN, 0], [600, i64::MAX]]),
            "GW2EI's long.MinValue/MaxValue sentinels survive as exact i64s"
        );
        assert_eq!(crd["iconURL"], "https://i.imgur.com/RiCJalE.png", "Daredevil");
        // MENEMYPROF: the enemy PLAYER target now resolves its OWN spec icon
        // (this fixture's enemy is a Reaper), where before it always got
        // `UNKNOWN_PROFESSION_ICON` because `model::Enemy` carried no
        // profession at all. MROSTER: there is no NPC target left to check
        // the "NPCs get no replay block" half against -- the curated roster
        // is enemy players only, which is exactly GW2EI's own WvW shape (its
        // 56 enemy-player targets all carry `combatReplayData`).
        let targets = v["targets"].as_array().unwrap();
        assert!(targets.iter().all(|t| t["enemyPlayer"] == true), "curated roster is enemy players only");
        let enemy = targets.iter().find(|t| t["enemyPlayer"] == true).unwrap();
        assert_eq!(
            enemy["combatReplayData"]["iconURL"],
            axilog_core::icons::prof_icon_url("Necromancer", "Reaper"),
            "enemy player resolves its own elite-spec icon, not the unknown fallback"
        );
        assert_eq!(enemy["profession"], "Reaper", "targets[].profession carries the spec");
    }

    /// Corrected audit fix: an enemy-player track with an EMPTY `positions`
    /// list (the `NonSquadPlayer`/no-`forcePolling` case -- see
    /// `axilog_core::analysis::ei_replay::build_world_track`'s doc comment)
    /// still gets a `combatReplayData` object -- GW2EI's own
    /// `JsonActorBuilder.cs:103-104` builds it unconditionally whenever
    /// replay was requested at all (`SingleActorCombatReplayDescription`'s
    /// ctor assigns `Positions`/`Rotations` straight from whatever the
    /// actor polled, empty or not); it does NOT gate on
    /// `HasCombatReplayPositions`. Verified against the local reference
    /// export's "Dummy PvP Agent" target, which carries a full
    /// `combatReplayData` with `positions: []`/`orientations: []` but real
    /// `start`/`end`/`iconURL`/`dead`/`down`/`dc`. What must stay fixed is
    /// the CONTENT: no `(int.MinValue, int.MinValue)`-derived sentinel
    /// coordinates for an enemy with zero raw position events (that was the
    /// actual bug, fixed in `build_world_track`'s `poll` call) -- just a
    /// real, empty `positions` array, not an omitted object.
    #[test]
    fn enemy_target_with_no_polled_positions_still_gets_combat_replay_data() {
        use axilog_core::analysis::ei_replay::{EiReplay, EiTrack};
        let report = sample_report();
        let squad_track = |name: &str| EiTrack {
            agent_addr: 1, name: name.to_string(), is_squad: true, start: 0, end: 600,
            positions: vec![[1.5, 2.5]], orientations: vec![90.0], dc: vec![], down: vec![], dead: vec![],
        };
        // `sample_report()` has 2 players and 1 enemy PLAYER (id 9); the
        // gadget (id 10) is not in the curated EI roster at all. the
        // enemy's own track has NO polled positions at all (correctly, per
        // the `forcePolling` fix -- not the pre-fix sentinel garbage).
        let replay = EiReplay {
            tracks: vec![
                squad_track("A"),
                squad_track("B"),
                EiTrack {
                    agent_addr: 9, name: "Foe".into(), is_squad: false, start: 0, end: 600,
                    positions: vec![], orientations: vec![],
                    dc: vec![[i64::MIN, 0], [600, i64::MAX]], down: vec![], dead: vec![],
                },
            ],
            map_id: Some(38),
            meta: None,
        };
        let v = to_ei_json(&sample_report_v1(), &report, &EiInputs { replay: Some(&replay), ..Default::default() });
        let enemy = v["targets"].as_array().unwrap().iter().find(|t| t["enemyPlayer"] == true).unwrap();
        let crd = enemy.get("combatReplayData").expect("combatReplayData must always be present when replay is on");
        assert_eq!(crd["positions"], json!([]), "empty, not omitted");
        assert_eq!(crd["orientations"], json!([]));
        assert_eq!(crd["start"], 0);
        assert_eq!(crd["end"], 600);
        assert_eq!(crd["dc"], json!([[i64::MIN, 0], [600, i64::MAX]]));
        assert_eq!(crd["down"], json!([]));
        assert_eq!(crd["dead"], json!([]));
        // MENEMYPROF: resolved from the enemy's own spec now, not the
        // unknown-spec fallback. An EMPTY position list does not affect icon
        // resolution -- the two are independent.
        assert_eq!(crd["iconURL"], axilog_core::icons::prof_icon_url("Necromancer", "Reaper"));
        // No sentinel garbage: every coordinate must be absent, not just
        // finite -- this is what the real bug produced (~[-2.6e10, 1.9e10]
        // pixel coordinates derived from the int.MinValue world sentinel).
        assert!(crd["positions"].as_array().unwrap().is_empty());
        // Squad players are unaffected by any of this.
        assert!(v["players"][0]["combatReplayData"]["positions"].as_array().is_some());
    }

    /// M15 Task 3: a track list that does not line up with the report's own
    /// roster is DROPPED rather than mis-attributed (the positional-join
    /// guard at the top of `to_ei_json`).
    #[test]
    fn mismatched_replay_track_count_is_ignored_not_misattributed() {
        use axilog_core::analysis::ei_replay::{EiReplay, EiTrack};
        let replay = EiReplay {
            tracks: vec![EiTrack {
                agent_addr: 1, name: "A".into(), is_squad: true, start: 0, end: 0,
                positions: vec![], orientations: vec![], dc: vec![], down: vec![], dead: vec![],
            }],
            map_id: Some(38),
            meta: None,
        };
        let v = to_ei_json(&sample_report_v1(), &sample_report(), &EiInputs { replay: Some(&replay), ..Default::default() });
        assert!(v["players"][0]["combatReplayData"].get("positions").is_none());
    }
}
