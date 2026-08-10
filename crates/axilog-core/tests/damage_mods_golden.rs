//! M16: damage-modifier calibration against a real Elite Insights export.
//!
//! Task 1 calibrated the ONE definition it shipped (`Moving Bonus`, `d10`).
//! Task 2 turns that into the milestone's actual proof: **every catalogued
//! modifier id, on every joined account**, compared field-for-field against
//! the reference export's `damageModifiers` / `incomingDamageModifiers`
//! arrays.
//!
//! Reading the table it prints:
//!
//! Two classes of id are held to different standards, deliberately:
//!
//! - **buff-free** modifiers (GW2EI's `DamageLogDamageModifier` and
//!   `SkillDamageModifier`: a flag on the damage row, no buff state) are
//!   asserted **EXACT** on all four fields for every account. Everything in
//!   this class is entirely under M16's control -- eligibility, gain
//!   formula, rounding, denominator -- so any drift is a real regression.
//! - **buff-gated** modifiers additionally depend on the per-`(actor, buff)`
//!   STACK COUNT at hit time, which comes from M3's buff simulator. That
//!   simulator is calibrated to a tolerance, not exactly (see
//!   `boons_golden.rs`: 2pp presence, 5% relative average stacks, with a
//!   documented Stability `StackingConditionalLoss` gap), and it has never
//!   been calibrated at all for non-boon buffs, which have no golden
//!   surface anywhere in EI's JSON. Those ids are therefore asserted to a
//!   documented per-id tolerance and their exact-row counts are printed, so
//!   the residual is visible and cannot silently grow.
//!
//! - a row exists in the export only when GW2EI recorded at least one
//!   qualifying hit (`JsonDamageModifierDataBuilder.cs:43-76`), and this
//!   project follows the same rule, so an id absent from BOTH sides is not
//!   a mismatch -- it is agreement,
//! - an id the catalog does not carry (see `analysis::damage_mods::catalog`'s
//!   skipped-definition table) is reported as UNCOVERED, never silently
//!   ignored,
//! - everything else is compared EXACT.
//!
//! The reference pair is the gitignored, PII-bearing local capture
//! (`fixtures/local/wvw-postrework.{zevtc,ei.json}`); the test SKIPS when
//! either half is absent, so CI is unaffected. Point `AXILOG_LOCAL_FIXTURES`
//! at the primary checkout to run it from a worktree:
//!
//! ```sh
//! AXILOG_LOCAL_FIXTURES=/path/to/axilog/fixtures/local \
//!     cargo test -p axilog-core --test damage_mods_golden -- --nocapture
//! ```

use axilog_core::analysis::damage::InstidRegistry;
use axilog_core::analysis::damage_mods::{catalog, evaluate_catalog, DamageModifierStat};
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use std::collections::{BTreeMap, BTreeSet, HashMap};

mod common;

/// GW2EI `Mod_MovingBonus` -- `DamageModifierIDs.cs:23`.
const MOVING_BONUS_ID: i32 = 10;

/// `damageGain` is the only non-integer field. Both sides are the result of
/// `Math.Round(x, 3)` over an `f64` accumulation, so the comparison is
/// EXACT-to-the-emitted-digit with only a float-representation epsilon:
/// half a unit in the last place GW2EI serialises.
const GAIN_EPSILON: f64 = 5e-4;

/// The four numbers, as the export spells them.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Golden {
    hit_count: i64,
    total_hit_count: i64,
    damage_gain: f64,
    total_damage: f64,
}

/// Every `(signed id -> phase-0 row)` for one golden player, merging the
/// outgoing and incoming arrays (the ids are already signed, so they cannot
/// collide -- `DamageModifier.cs:26`).
fn golden_rows(p: &serde_json::Value) -> BTreeMap<i32, Golden> {
    let mut out = BTreeMap::new();
    for key in ["damageModifiers", "incomingDamageModifiers"] {
        let Some(arr) = p.get(key).and_then(|v| v.as_array()) else { continue };
        for m in arr {
            let Some(id) = m["id"].as_i64() else { continue };
            let Some(row) = m.get("damageModifiers").and_then(|v| v.as_array()).and_then(|v| v.first())
            else {
                continue;
            };
            out.insert(id as i32, Golden {
                hit_count: row["hitCount"].as_i64().unwrap_or(0),
                total_hit_count: row["totalHitCount"].as_i64().unwrap_or(0),
                damage_gain: row["damageGain"].as_f64().unwrap_or(0.0),
                total_damage: row["totalDamage"].as_f64().unwrap_or(0.0),
            });
        }
    }
    out
}

fn matches(ours: &DamageModifierStat, g: &Golden) -> Vec<String> {
    let mut bad = Vec::new();
    if ours.hit_count as i64 != g.hit_count {
        bad.push(format!("hitCount ours={} golden={}", ours.hit_count, g.hit_count));
    }
    if ours.total_hit_count as i64 != g.total_hit_count {
        bad.push(format!(
            "totalHitCount ours={} golden={}",
            ours.total_hit_count, g.total_hit_count
        ));
    }
    if (ours.damage_gain - g.damage_gain).abs() > GAIN_EPSILON {
        bad.push(format!(
            "damageGain ours={:.3} golden={:.3}",
            ours.damage_gain, g.damage_gain
        ));
    }
    if ours.total_damage as f64 != g.total_damage {
        bad.push(format!(
            "totalDamage ours={} golden={:.0}",
            ours.total_damage, g.total_damage
        ));
    }
    bad
}

/// One row of the per-id calibration contract.
///
/// Every id the catalog can produce for this log has exactly one entry, and
/// the entry is either "**every** row matched GW2EI on **all four** fields
/// for **every** account" ([`IdBound::exact`]) or an explicit per-FIELD
/// bound on the aggregate residual across all joined accounts
/// ([`IdBound::within`]), seeded from the measured value with 20% headroom
/// and carrying the measurement in a comment above it.
///
/// Why per-field and not one global slush:
///
/// - a single `hitCount` bound is **vacuous** for the `ByStack` /
///   `ByMultiPresence` family, where `hitCount` is right by construction
///   (any non-zero stack count qualifies the hit) and the entire error is in
///   `damageGain`. `d174` Empowered aggregates to 967 hits vs GW2EI's 967 --
///   residual exactly 0.0 -- while none of its five rows is exact; same for
///   `d111`. Bounding only `hitCount` would have tested nothing on them.
/// - one shared constant sized for the worst id is ~4x looser than every
///   other id needs, so a genuine regression could multiply the Stability
///   residual several-fold without tripping anything.
///
/// A field whose bound is `0.0` must agree with GW2EI EXACTLY in aggregate
/// even though individual rows may differ (they cancel) -- that is the
/// strongest statement available for it, and it is deliberately pinned.
#[derive(Clone, Copy)]
struct IdBound {
    id: i32,
    /// Every row exact on all four fields; the bounds are then unused.
    rows_exact: bool,
    /// Relative aggregate bound per field, in [`Tally::residuals`] order.
    bounds: [f64; 4],
}

impl IdBound {
    const fn exact(id: i32) -> Self {
        IdBound { id, rows_exact: true, bounds: [0.0; 4] }
    }
    const fn within(id: i32, bounds: [f64; 4]) -> Self {
        IdBound { id, rows_exact: false, bounds }
    }
}

/// `Tally::residuals` order.
const FIELD_NAMES: [&str; 4] = ["hitCount", "totalHitCount", "damageGain", "totalDamage"];

/// The measured calibration contract: 69 ids, 30 of them exact on every row
/// of every account, 39 carrying a residual.
///
/// The residuals have two distinct, independently tracked causes; neither is
/// a damage-modifier defect, and conflating them would let a future fix for
/// one claim credit for the other:
///
/// 1. **Buff-state fidelity** (37 ids). Buff-gated modifiers are only as
///    exact as the per-`(actor, buff)` stack timelines underneath them. This
///    project's buff simulator reproduces GW2EI's own uptimes on this export
///    to ~0.01pp, but the stack-count fine structure is approximate --
///    `boons_golden.rs` already records that (2pp presence, 5% relative
///    average stacks, with a named Stability `StackingConditionalLoss`
///    allowlist), and NO non-boon buff has ever been calibrated here at all,
///    because EI's JSON exposes no per-buff timeline to calibrate against.
///    The three widest ids are the most stack-sensitive ones:
///    - `d422` "Might 25" uses `GainComputerByExactNumberOfBuffsPresent(25)`,
///      so it only fires at SATURATION -- a simulation that sits one stack
///      low still matches `Might >= 20` (`d423`, residual 0.0014) and average
///      stacks to ~1% while missing most "exactly 25".
///    - `d312` Relic of Fireworks and `d369` Chant of Action watch
///      `BuffStackType.Force` buffs (`Buff.cs:120`, capacity 1). The
///      mechanism is NOT yet isolated: `buffs::simulator`'s `capacity == 1`
///      branch does replace the held stack (`simulator.rs:196-201`), which
///      matches `ForceOverrideLogic`, so the earlier "no refresh on
///      re-application" explanation is wrong. All that is established is
///      that it is buff-state, not modifier logic -- the same eligible-hit
///      pools match exactly.
/// 2. **An incoming-damage attribution gap** (2 ids are dominated by it,
///    `d-126` and `d-62`, and it perturbs every incoming id slightly). See
///    [`INCOMING_DEFICIT_ACCOUNTS`] -- it is a denominator/attribution
///    difference on ONE account, which no buff simulator can cause or fix.
#[rustfmt::skip]
const ID_BOUNDS: &[IdBound] = &[
    // measured: 0.000398 0.000000 0.001184 0.000069
    IdBound::within(-431, [0.000478, 0.0, 0.001421, 0.000083]),
    // measured: 0.078921 0.001265 0.096984 0.000080
    IdBound::within(-428, [0.094706, 0.001518, 0.116381, 0.000096]),
    // measured: 0.073823 0.001076 0.143148 0.000069
    IdBound::within(-427, [0.088588, 0.001292, 0.171778, 0.000083]),
    // measured: 0.028726 0.001076 0.041476 0.000069
    IdBound::within(-426, [0.034472, 0.001292, 0.049772, 0.000083]),
    // measured: 0.001291 0.001076 0.002553 0.000069
    IdBound::within(-425, [0.001550, 0.001292, 0.003064, 0.000083]),
    IdBound::exact(-411),
    IdBound::exact(-390),
    // measured: 0.000000 0.002721 0.000000 0.000069
    IdBound::within(-389, [0.0, 0.003266, 0.0, 0.000083]),
    IdBound::exact(-376),
    IdBound::exact(-370),
    // measured: 0.000000 0.000000 0.000000 0.000312
    IdBound::within(-368, [0.0, 0.0, 0.0, 0.000375]),
    // measured: 0.012931 0.016355 0.005230 0.000376
    IdBound::within(-336, [0.015518, 0.019626, 0.006276, 0.000452]),
    IdBound::exact(-176),
    // measured: 0.002227 0.000000 0.001043 0.000000
    IdBound::within(-132, [0.002673, 0.0, 0.001252, 0.0]),
    IdBound::exact(-129),
    IdBound::exact(-128),
    // measured: 0.000000 0.023569 0.000000 0.001174
    IdBound::within(-126, [0.0, 0.028283, 0.0, 0.001409]),
    IdBound::exact(-99),
    IdBound::exact(-94),
    IdBound::exact(-93),
    IdBound::exact(-78),
    // measured: 0.000000 0.007910 0.000000 0.000174
    IdBound::within(-62, [0.0, 0.009492, 0.0, 0.000209]),
    IdBound::exact(-61),
    // measured: 0.002643 0.002721 0.001042 0.000069
    IdBound::within(-59, [0.003172, 0.003266, 0.001251, 0.000083]),
    // measured: 0.000508 0.000000 0.001996 0.000069
    IdBound::within(-58, [0.000610, 0.0, 0.002396, 0.000083]),
    // measured: 0.033191 0.001305 0.059840 0.000071
    IdBound::within(-57, [0.039830, 0.001566, 0.071808, 0.000086]),
    // measured: 0.000000 0.000000 0.000000 0.000108
    IdBound::within(-54, [0.0, 0.0, 0.0, 0.000130]),
    IdBound::exact(10),
    IdBound::exact(11),
    IdBound::exact(18),
    IdBound::exact(21),
    IdBound::exact(25),
    IdBound::exact(36),
    // measured: 0.019108 0.000000 0.174279 0.000000
    IdBound::within(44, [0.022930, 0.0, 0.209135, 0.0]),
    // measured: 0.002380 0.000000 0.051772 0.000000
    IdBound::within(67, [0.002856, 0.0, 0.062127, 0.0]),
    // measured: 0.006051 0.000000 0.013848 0.000000
    IdBound::within(74, [0.007262, 0.0, 0.016618, 0.0]),
    // measured: 0.006818 0.000000 0.000573 0.000000
    IdBound::within(75, [0.008182, 0.0, 0.000688, 0.0]),
    IdBound::exact(93),
    IdBound::exact(98),
    // measured: 0.015158 0.000000 0.033767 0.000000
    IdBound::within(107, [0.018190, 0.0, 0.040521, 0.0]),
    // measured: 0.001573 0.000000 0.002285 0.000000
    IdBound::within(108, [0.001888, 0.0, 0.002742, 0.0]),
    // measured: 0.004167 0.000000 0.002758 0.000000
    IdBound::within(109, [0.005001, 0.0, 0.003310, 0.0]),
    // measured: 0.000000 0.000000 0.000447 0.000000
    IdBound::within(111, [0.0, 0.0, 0.000537, 0.0]),
    IdBound::exact(119),
    // measured: 0.001255 0.000000 0.003163 0.000000
    IdBound::within(124, [0.001506, 0.0, 0.003796, 0.0]),
    // measured: 0.001701 0.000000 0.003175 0.000000
    IdBound::within(125, [0.002042, 0.0, 0.003810, 0.0]),
    // measured: 0.025641 0.000000 0.035714 0.000000
    IdBound::within(131, [0.030770, 0.0, 0.042857, 0.0]),
    IdBound::exact(170),
    // measured: 0.028646 0.000000 0.063441 0.000000
    IdBound::within(172, [0.034376, 0.0, 0.076130, 0.0]),
    // measured: 0.006579 0.000000 0.004592 0.000000
    IdBound::within(173, [0.007895, 0.0, 0.005511, 0.0]),
    // measured: 0.000000 0.000000 0.005516 0.000000
    IdBound::within(174, [0.0, 0.0, 0.006620, 0.0]),
    IdBound::exact(175),
    // measured: 0.446185 0.000000 0.392927 0.000000
    IdBound::within(312, [0.535422, 0.0, 0.471513, 0.0]),
    IdBound::exact(313),
    // measured: 0.010601 0.000000 0.001767 0.000000
    IdBound::within(318, [0.012722, 0.0, 0.002121, 0.0]),
    IdBound::exact(319),
    // measured: 0.000000 0.000000 0.000200 0.000000
    IdBound::within(334, [0.0, 0.0, 0.000240, 0.0]),
    IdBound::exact(361),
    IdBound::exact(362),
    IdBound::exact(364),
    // measured: 0.372093 0.000000 0.359495 0.000000
    IdBound::within(369, [0.446512, 0.0, 0.431394, 0.0]),
    // measured: 0.004566 0.000000 0.005170 0.000000
    IdBound::within(371, [0.005480, 0.0, 0.006204, 0.0]),
    // measured: 0.013636 0.000000 0.021268 0.000000
    IdBound::within(372, [0.016364, 0.0, 0.025522, 0.0]),
    IdBound::exact(374),
    IdBound::exact(403),
    // measured: 0.642352 0.018953 0.649957 0.003220
    IdBound::within(422, [0.770823, 0.022744, 0.779949, 0.003865]),
    // measured: 0.001363 0.000000 0.002842 0.000000
    IdBound::within(423, [0.001636, 0.0, 0.003411, 0.0]),
    // measured: 0.004245 0.000000 0.011645 0.000000
    IdBound::within(424, [0.005094, 0.0, 0.013974, 0.0]),
    // measured: 0.001134 0.000000 0.003709 0.000000
    IdBound::within(429, [0.001361, 0.0, 0.004451, 0.0]),
];


/// **Tracked cause 2: an incoming-damage attribution gap on one account.**
///
/// Exactly one of the 44 joined accounts is short a fixed slice of the
/// damage it TOOK, and the shortfall shows up identically on all 14 of that
/// account's incoming rows -- every row short exactly `239` `totalDamage`,
/// and the ten rows whose `src_type` admits conditions additionally short
/// exactly `7` `totalHitCount`. The strike-only rows are short the damage
/// but not the hits, which pins the missing rows down to **7 incoming
/// CONDITION ticks totalling 239 damage** that GW2EI attributes to this
/// player and this project does not.
///
/// This is not something a buff simulator can cause or fix: it is the
/// eligible-hit pool and the denominator, upstream of any gain computation.
/// It is bounded here rather than fixed because it is not a
/// damage-modifier defect either -- the same 7 rows are missing from the
/// underlying damage-taken pool.
///
/// **Hypothesis, not a finding:** this smells like the same family as the
/// `dst_agent == 0` / orphaned-instid rows Task 1's `NonZeroAddrIndex`
/// repairs (see `analysis::damage_mods`'s zero-addr note, and that type's
/// own "unbounded lookback" and "no `FirstAware`/`LastAware` window"
/// follow-ups). It has NOT been traced to those rows, and it should not be
/// assumed fixed by anything that touches them.
///
/// The assertion is deliberately structural rather than by account name:
/// at most this many accounts may show an incoming denominator deficit, and
/// no single row may be short more than the measured amounts. No account
/// identifier is committed.
const INCOMING_DEFICIT_ACCOUNTS: usize = 1;
/// Per-row bound for the account above: `7` hits and `239` damage.
const INCOMING_DEFICIT_PER_ROW: (i64, f64) = (7, 239.0);

/// Per-id tally over all joined accounts.
#[derive(Default, Clone, Copy)]
struct Tally {
    exact: u32,
    mismatched: u32,
    /// Rows this project produced that the export does not have at all.
    extra: u32,
    /// Rows the export has that this project did not produce.
    missing: u32,
    /// Aggregate hit counts, for the buff-gated tolerance.
    our_hits: u64,
    golden_hits: u64,
    our_total_hits: u64,
    golden_total_hits: u64,
    our_gain: f64,
    golden_gain: f64,
    our_damage: u64,
    golden_damage: f64,
}

impl Tally {
    fn add_ours(&mut self, o: &DamageModifierStat) {
        self.our_hits += u64::from(o.hit_count);
        self.our_total_hits += u64::from(o.total_hit_count);
        self.our_gain += o.damage_gain;
        self.our_damage += o.total_damage;
    }
    fn add_golden(&mut self, g: &Golden) {
        self.golden_hits += g.hit_count.max(0) as u64;
        self.golden_total_hits += g.total_hit_count.max(0) as u64;
        self.golden_gain += g.damage_gain;
        self.golden_damage += g.total_damage;
    }
    /// Relative aggregate residual per field, in the declaration order of
    /// [`IdBound`]: hitCount, totalHitCount, damageGain, totalDamage.
    fn residuals(&self) -> [f64; 4] {
        fn rel(a: f64, b: f64) -> f64 {
            let d = (a - b).abs();
            if d <= 1e-6 { 0.0 } else { d / b.abs().max(1.0) }
        }
        [
            rel(self.our_hits as f64, self.golden_hits as f64),
            rel(self.our_total_hits as f64, self.golden_total_hits as f64),
            rel(self.our_gain, self.golden_gain),
            rel(self.our_damage as f64, self.golden_damage),
        ]
    }
}

#[test]
fn catalog_matches_the_local_reference_export_when_available() {
    let zevtc = common::local_fixture("wvw-postrework.zevtc");
    let ei_json = common::local_fixture("wvw-postrework.ei.json");
    let Some(bytes) = std::fs::read(&zevtc).ok() else {
        println!("skip: {zevtc} absent (damage-modifier calibration)");
        return;
    };
    let Some(golden_s) = std::fs::read_to_string(&ei_json).ok() else {
        println!("skip: {ei_json} absent (damage-modifier calibration)");
        return;
    };
    let golden: serde_json::Value =
        serde_json::from_str(&golden_s).unwrap_or_else(|e| panic!("parse {ei_json}: {e}"));

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let registry = InstidRegistry::build(&raw);
    let stats = evaluate_catalog(&raw, &registry, &enc);

    // Sanity: the export must actually carry the descriptor table, or this
    // test is vacuous.
    let descriptors = golden["damageModMap"].as_object().expect("damageModMap");
    assert!(
        descriptors.contains_key("d10"),
        "reference export has no damageModMap.d10 -- calibration target missing"
    );

    // Which ids CAN this catalog produce for a log of this era and mode?
    // (`available` + `keep`, the same two gates `evaluate` applies.)
    let ctx = axilog_core::analysis::damage_mods::ModeContext::from_encounter(&enc);
    let gw2 = axilog_core::analysis::damage_mods::gw2_build(&raw);
    let evtc = axilog_core::analysis::damage_mods::evtc_build(&raw);
    let active: Vec<&&axilog_core::analysis::damage_mods::model::DamageModifierDef> = catalog::CATALOG
        .iter()
        .filter(|d| d.available(gw2, evtc) && d.keep(ctx.parse_mode, ctx.skill_mode))
        .collect();
    let covered: BTreeSet<i32> = active.iter().map(|d| d.json_id()).collect();
    // Buff-free == GW2EI's `DamageLogDamageModifier`/`SkillDamageModifier`:
    // no tracker, and no checker that consults a buff timeline either.
    let buff_free: BTreeSet<i32> = active
        .iter()
        .filter(|d| d.trackers().is_empty() && d.checks.iter().all(|c| c.buff_id().is_none()))
        .map(|d| d.json_id())
        .collect();

    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden["players"].as_array().expect("players array") {
        if let Some(a) = p["account"].as_str() {
            golden_by_account.insert(common::account_key(a).to_string(), p);
        }
    }

    let mut tallies: BTreeMap<i32, Tally> = BTreeMap::new();
    let mut mismatches: Vec<String> = Vec::new();
    let mut joined = 0usize;
    let mut moving_bonus_rows = 0usize;
    // Tracked cause 2: the worst per-ROW incoming denominator deficit seen
    // on each account (see `INCOMING_DEFICIT_ACCOUNTS`).
    let mut per_account_incoming: BTreeMap<String, (i64, f64, u32)> = BTreeMap::new();

    for p in &enc.players {
        let key = common::account_key(&p.account).to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        joined += 1;
        let rows = golden_rows(golden_p);
        let ours: BTreeMap<i32, DamageModifierStat> = stats
            .iter()
            .filter(|((addr, _), _)| *addr == p.agent_addr)
            .map(|((_, id), s)| (*id, *s))
            .collect();

        let ids: BTreeSet<i32> =
            rows.keys().copied().chain(ours.keys().copied()).filter(|id| covered.contains(id)).collect();
        for id in ids {
            let t = tallies.entry(id).or_default();
            match (rows.get(&id), ours.get(&id)) {
                (Some(g), Some(o)) => {
                    t.add_ours(o);
                    t.add_golden(g);
                    if id < 0 {
                        let d_hits = g.total_hit_count - i64::from(o.total_hit_count);
                        let d_dmg = g.total_damage - o.total_damage as f64;
                        if d_hits != 0 || d_dmg != 0.0 {
                            let e = per_account_incoming.entry(key.clone()).or_default();
                            e.0 = e.0.max(d_hits.abs());
                            e.1 = e.1.max(d_dmg.abs());
                            e.2 += 1;
                        }
                    }
                    let bad = matches(o, g);
                    if bad.is_empty() {
                        t.exact += 1;
                    } else {
                        t.mismatched += 1;
                        if buff_free.contains(&id) {
                            mismatches.push(format!("d{id} {key}: {}", bad.join(", ")));
                        }
                    }
                    if id == MOVING_BONUS_ID {
                        moving_bonus_rows += 1;
                    }
                }
                (Some(g), None) => {
                    t.missing += 1;
                    t.add_golden(g);
                    if buff_free.contains(&id) {
                        mismatches.push(format!(
                            "d{id} {key}: MISSING -- golden has {}/{} hits",
                            g.hit_count, g.total_hit_count
                        ));
                    }
                }
                (None, Some(o)) => {
                    t.extra += 1;
                    t.add_ours(o);
                    // An EXTRA row is a structural error in ANY class: it
                    // means the modifier was offered to an actor GW2EI never
                    // offered it to (spec gating, mode gating or era gating),
                    // not a stack-count difference.
                    mismatches.push(format!(
                        "d{id} {key}: EXTRA -- we produced {}/{} hits, golden has no row",
                        o.hit_count, o.total_hit_count
                    ));
                }
                (None, None) => unreachable!("id came from one of the two maps"),
            }
        }
    }

    // Coverage table, keyed by the descriptor ids the export itself carries
    // -- so an id we cover but the export never mentions cannot inflate it.
    let exported: BTreeSet<i32> = descriptors
        .keys()
        .filter_map(|k| k.strip_prefix('d').and_then(|s| s.parse::<i32>().ok()))
        .collect();
    let mut covered_ids = 0usize;
    let (mut rows_exact, mut rows_total) = (0u32, 0u32);
    let (mut free_ids, mut free_exact_ids) = (0usize, 0usize);
    let mut over_tolerance: Vec<String> = Vec::new();
    println!(
        "\n{:<8} {:<6} {:<7} {:<9} {:<7} {:<7}  name",
        "id", "class", "exact", "mismatch", "extra", "missing"
    );
    for id in &exported {
        let name = descriptors
            .get(&format!("d{id}"))
            .and_then(|v| v["name"].as_str())
            .unwrap_or("?");
        let class = if buff_free.contains(id) { "flag" } else { "buff" };
        match tallies.get(id) {
            Some(t) => {
                covered_ids += 1;
                rows_exact += t.exact;
                rows_total += t.exact + t.mismatched + t.extra + t.missing;
                if buff_free.contains(id) {
                    free_ids += 1;
                    if t.mismatched + t.extra + t.missing == 0 {
                        free_exact_ids += 1;
                    }
                }
                match ID_BOUNDS.iter().find(|b| b.id == *id) {
                    None => over_tolerance.push(format!(
                        "d{id} ({name}): produced rows but has no ID_BOUNDS entry -- \
                         add one (measured residuals: {:?})",
                        t.residuals()
                    )),
                    Some(b) if b.rows_exact => {
                        if t.mismatched + t.extra + t.missing > 0 {
                            over_tolerance.push(format!(
                                "d{id} ({name}): declared EXACT but {} row(s) differ \
                                 ({} mismatched, {} extra, {} missing)",
                                t.mismatched + t.extra + t.missing,
                                t.mismatched,
                                t.extra,
                                t.missing
                            ));
                        }
                    }
                    Some(b) => {
                        let r = t.residuals();
                        for f in 0..4 {
                            if r[f] > b.bounds[f] + 1e-9 {
                                over_tolerance.push(format!(
                                    "d{id} ({name}): aggregate {} residual {:.6} exceeds \
                                     bound {:.6}",
                                    FIELD_NAMES[f], r[f], b.bounds[f]
                                ));
                            }
                        }
                    }
                }
                println!(
                    "d{:<7} {class:<6} {:<7} {:<9} {:<7} {:<7}  {name}",
                    id, t.exact, t.mismatched, t.extra, t.missing
                );
            }
            None if covered.contains(id) => {
                covered_ids += 1;
                println!(
                    "d{:<7} {class:<6} {:<7} {:<9} {:<7} {:<7}  {name} (no rows either side)",
                    id, 0, 0, 0, 0
                );
            }
            None => println!("d{id:<7} {:^41}  {name}", "UNCOVERED"),
        }
    }
    let declared_exact = ID_BOUNDS.iter().filter(|b| b.rows_exact).count();
    println!(
        "\ncoverage: {covered_ids}/{} exported ids, {joined} account(s) joined\n\
         rows: {rows_exact}/{rows_total} exact\n\
         ids exact on every row of every account: {declared_exact}/{} \
         (of which buff-free: {free_exact_ids}/{free_ids})",
        exported.len(),
        ID_BOUNDS.len()
    );

    // Tracked cause 2 (see `INCOMING_DEFICIT_ACCOUNTS`): bound the
    // incoming-damage attribution gap structurally, so it cannot spread to
    // more accounts or grow on the one it affects.
    let mut deficit_failures: Vec<String> = Vec::new();
    if per_account_incoming.len() > INCOMING_DEFICIT_ACCOUNTS {
        deficit_failures.push(format!(
            "{} account(s) show an incoming denominator deficit, expected at most {}",
            per_account_incoming.len(),
            INCOMING_DEFICIT_ACCOUNTS
        ));
    }
    for (i, (_, (d_hits, d_dmg, rows))) in per_account_incoming.iter().enumerate() {
        println!(
            "incoming denominator deficit, account #{i}: {rows} row(s), \
             worst dTotalHitCount={d_hits}, worst dTotalDamage={d_dmg}"
        );
        if *d_hits > INCOMING_DEFICIT_PER_ROW.0 || *d_dmg > INCOMING_DEFICIT_PER_ROW.1 {
            deficit_failures.push(format!(
                "account #{i}: per-row incoming deficit ({d_hits} hits, {d_dmg} damage) \
                 exceeds the measured ({}, {})",
                INCOMING_DEFICIT_PER_ROW.0, INCOMING_DEFICIT_PER_ROW.1
            ));
        }
    }


    // Guards against the harness silently degrading.
    assert!(joined >= 44, "only {joined} account(s) joined -- expected at least 44");
    assert_eq!(
        moving_bonus_rows, 44,
        "Moving Bonus (d10) must still join on all 44 accounts it did in Task 1"
    );
    assert!(
        covered_ids >= 40,
        "only {covered_ids} of the export's {} ids are covered -- the catalog regressed",
        exported.len()
    );
    assert_eq!(
        free_exact_ids, free_ids,
        "every buff-free modifier id must be exact on every account"
    );
    assert!(
        deficit_failures.is_empty(),
        "incoming-damage attribution gap changed:\n{}",
        deficit_failures.join("\n")
    );
    assert!(
        over_tolerance.is_empty(),
        "per-id calibration contract violated (re-measure and re-seed ID_BOUNDS \
         only after establishing WHY each number moved):\n{}",
        over_tolerance.join("\n")
    );
    assert!(
        mismatches.is_empty(),
        "damage-modifier mismatches (buff-free rows, or structural EXTRA rows):\n{}",
        mismatches.join("\n")
    );
}
