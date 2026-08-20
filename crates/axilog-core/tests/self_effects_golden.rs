//! `analysis::self_effects` calibration against a real Elite Insights
//! export.
//!
//! Elite Insights carries every buff a player HELD in one `buffUptimes`
//! array per player -- boons, conditions and control effects alike -- with
//! `buffData[0].uptime` and a `states` step timeline per entry. That is the
//! same measurement this pass produces, split out by family, so every value
//! here is computed twice and asserted to agree.
//!
//! Reference: `fixtures/local/wvw-postrework.{zevtc,ei.json}`, gitignored
//! real capture data. Skips cleanly when absent, honouring
//! `AXILOG_LOCAL_FIXTURES` -- the same pattern every `*_ei_golden.rs` file
//! uses.
//!
//! Field mapping, from `analysis::buffs::uptime`'s module doc (verified
//! against GW2EI source there, not guessed): for a DURATION buff EI's
//! `buffData[0].uptime` is the percentage of the phase with the buff
//! active, which is this pass's `presence_pct`, and EI's `presence` field
//! is never populated. For an INTENSITY buff `uptime` is a time-weighted
//! mean stack count -- this pass's `avg_stacks` -- and `presence` is the
//! percentage. Every one of the 6 `BuffStackType.Stacking` conditions
//! (`CommonBuffs.cs:36-40` + `:49`) is intensity; the other 10 tracked ids
//! are duration.

use axilog_core::analysis::self_effects::{self, effect_ids, effect_kind};
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use serde_json::Value;
use std::collections::BTreeMap;

/// Duration-effect uptime, in percentage points.
///
/// MEASURED on `fixtures/local/wvw-postrework.ei.json`: worst cell
/// 0.000500pp on buff 720 (Blind, account `BreakN.5496`), across 231
/// duration cells spanning 9 of the 10 duration ids -- Taunt (27705)
/// appears in no player's `buffUptimes` on this WvW capture and is
/// therefore UNCOVERED by this oracle. Across the 9 that are covered (720,
/// 721, 722, 727, 742, 791, 833, 872, 26766) each id's worst cell sat
/// between 0.000292 and 0.000500 -- i.e. every measured id already agrees
/// with EI to the full precision EI emits, and none is an outlier. The
/// bound below leaves ~10x margin over that.
///
/// The floor is 0.0005pp: Elite Insights rounds every emitted number
/// through `Math.Round(x, ParserHelper.BuffDigit)` with `BuffDigit = 3`
/// (`GW2EIEvtcParser/ParserHelpers/ParserHelper.cs:24`), whose maximum
/// representation error is exactly that. A tighter bound would be asserting
/// against precision the golden does not carry -- and since the measured
/// worst IS the floor, the residual here is entirely EI's serialization,
/// not a simulator disagreement.
const DURATION_TOLERANCE_PP: f64 = 0.005;

/// Intensity-effect average stacks, relative error. Same convention
/// `boons_golden.rs`'s `INTENSITY_STACK_RELATIVE_TOLERANCE` uses.
///
/// MEASURED on the same export: worst cell 0.000497 on buff 738
/// (Vulnerability, account `Gawna.6519`), across 245 intensity cells over
/// all 6 intensity ids (723, 736, 737, 738, 861, 19426), whose per-id worst
/// cells ranged 0.000467 (861) to 0.000497 (738). Unlike the duration
/// family, all 6 intensity ids are covered by this capture. The bound
/// leaves ~10x margin.
///
/// Because the comparison divides by `max(|theirs|, 1.0)` and nearly every
/// avg-stacks value on this capture is below 1.0, this relative bound is in
/// practice an absolute bound of 0.005 stacks -- which is again 10x the
/// 0.0005 `Math.Round(x, 3)` floor described on
/// [`DURATION_TOLERANCE_PP`], the same floor the measurement lands on.
const INTENSITY_TOLERANCE_REL: f64 = 0.005;

fn local_fixture(name: &str) -> String {
    let dir = std::env::var("AXILOG_LOCAL_FIXTURES")
        .unwrap_or_else(|_| format!("{}/../../fixtures/local", env!("CARGO_MANIFEST_DIR")));
    format!("{dir}/{name}")
}

fn account_key(account: &str) -> &str {
    account.trim_start_matches(':')
}

/// `(our SelfEffects, EI players[] by account key)`, or `None` when the
/// local capture is absent.
fn calibration() -> Option<(self_effects::SelfEffects, BTreeMap<u64, String>, Value)> {
    let zevtc = local_fixture("wvw-postrework.zevtc");
    let json = local_fixture("wvw-postrework.ei.json");
    let bytes = match std::fs::read(&zevtc) {
        Ok(b) => b,
        Err(_) => {
            println!("skip: {zevtc} absent (self-effects EI calibration)");
            return None;
        }
    };
    let text = match std::fs::read_to_string(&json) {
        Ok(s) => s,
        Err(_) => {
            println!("skip: {json} absent (self-effects EI calibration)");
            return None;
        }
    };
    let golden: Value = serde_json::from_str(&text).expect("parse EI export");
    let raw = decode_raw(&bytes).expect("decode capture");
    let enc = resolve(&raw);
    let ours = self_effects::build(&raw, &enc);
    // Representative agent address -> account key, the join this
    // calibration runs on. Both sides name the same accounts; EI's are
    // written without arcdps's leading colon.
    let accounts: BTreeMap<u64, String> =
        enc.players.iter().map(|p| (p.agent_addr, account_key(&p.account).to_string())).collect();
    Some((ours, accounts, golden))
}

/// One compared cell.
struct Cell {
    account: String,
    buff_id: u32,
    is_intensity: bool,
    ours: f64,
    theirs: f64,
}

/// Every (player, tracked id) EI reports, paired with our value for the
/// same key. A key EI has and we do not yields `ours = 0.0`, which is the
/// divergence the tolerance has to catch rather than skip.
fn cells(
    ours: &self_effects::SelfEffects,
    accounts: &BTreeMap<u64, String>,
    golden: &Value,
) -> Vec<Cell> {
    let ids = effect_ids();
    let by_account: BTreeMap<&str, u64> =
        accounts.iter().map(|(&addr, acc)| (acc.as_str(), addr)).collect();
    let mut out = Vec::new();
    for p in golden["players"].as_array().expect("players") {
        let account = account_key(p["account"].as_str().expect("account"));
        let Some(&addr) = by_account.get(account) else {
            // Skipped here, but NOT unaccounted for: this `continue` is
            // the one place the comparison can silently shrink, so
            // `the_key_set_matches_the_ei_export` asserts separately that
            // the only players it ever fires for are EI's `notInSquad`
            // entries -- every in-squad player must join.
            continue;
        };
        for b in p["buffUptimes"].as_array().into_iter().flatten() {
            let id = b["id"].as_u64().expect("buff id") as u32;
            if !ids.contains(&id) {
                continue;
            }
            let (is_intensity, _) = effect_kind(id).expect("tracked id");
            let theirs = b["buffData"][0]["uptime"].as_f64().unwrap_or(0.0);
            let mine = ours.uptime.get(&(addr, id));
            let ours_value = match (mine, is_intensity) {
                (Some(u), true) => u.avg_stacks,
                (Some(u), false) => u.presence_pct,
                (None, _) => 0.0,
            };
            out.push(Cell {
                account: account.to_string(),
                buff_id: id,
                is_intensity,
                ours: ours_value,
                theirs,
            });
        }
    }
    out
}

#[test]
fn report_worst_divergence_against_the_ei_export() {
    let Some((ours, accounts, golden)) = calibration() else { return };
    let cells = cells(&ours, &accounts, &golden);
    assert!(cells.len() > 200, "the comparison must not be vacuous, got {} cells", cells.len());

    let mut worst_duration = (0.0f64, String::new());
    let mut worst_intensity = (0.0f64, String::new());
    for c in &cells {
        let label = format!("{} buff {} ours={} theirs={}", c.account, c.buff_id, c.ours, c.theirs);
        if c.is_intensity {
            // Relative error, `boons_golden.rs`'s intensity convention.
            let rel = (c.ours - c.theirs).abs() / c.theirs.abs().max(1.0);
            if rel > worst_intensity.0 {
                worst_intensity = (rel, label);
            }
        } else {
            // Percentage points, `boons_golden.rs`'s duration convention.
            let pp = (c.ours - c.theirs).abs();
            if pp > worst_duration.0 {
                worst_duration = (pp, label);
            }
        }
    }
    println!("CELLS {}", cells.len());
    println!("WORST duration {:.6}pp  {}", worst_duration.0, worst_duration.1);
    println!("WORST intensity {:.6}rel  {}", worst_intensity.0, worst_intensity.1);

    // Per-id breakdown, so a single bad id cannot hide behind a good mean.
    let mut per_id: BTreeMap<u32, (f64, usize)> = BTreeMap::new();
    for c in &cells {
        let err = if c.is_intensity {
            (c.ours - c.theirs).abs() / c.theirs.abs().max(1.0)
        } else {
            (c.ours - c.theirs).abs()
        };
        let e = per_id.entry(c.buff_id).or_insert((0.0, 0));
        e.0 = e.0.max(err);
        e.1 += 1;
    }
    for (id, (worst, n)) in per_id {
        println!("ID {id} cells={n} worst={worst:.6}");
    }
}

#[test]
fn every_cell_agrees_with_the_ei_export() {
    let Some((ours, accounts, golden)) = calibration() else { return };
    let cells = cells(&ours, &accounts, &golden);
    assert!(cells.len() > 200, "the comparison must not be vacuous, got {} cells", cells.len());
    let mut failures: Vec<String> = Vec::new();
    for c in &cells {
        let ok = if c.is_intensity {
            (c.ours - c.theirs).abs() <= INTENSITY_TOLERANCE_REL * c.theirs.abs().max(1.0)
        } else {
            (c.ours - c.theirs).abs() <= DURATION_TOLERANCE_PP
        };
        if !ok {
            failures.push(format!(
                "{} buff {}: ours {} vs EI {}",
                c.account, c.buff_id, c.ours, c.theirs
            ));
        }
    }
    assert!(failures.is_empty(), "{} cells diverge:\n{}", failures.len(), failures.join("\n"));
}

/// Stun and Daze are the two ids this whole block exists for, and they are
/// the two with the fewest cells -- so they are exactly what a mean-based
/// check would hide. Measured on this capture: 11 players each, 37 state
/// pairs each. A pass that emitted nothing for them cannot go green here.
#[test]
fn stun_and_daze_are_covered_and_agree() {
    let Some((ours, accounts, golden)) = calibration() else { return };
    let cells = cells(&ours, &accounts, &golden);
    for id in [872u32, 833] {
        let mine: Vec<&Cell> = cells.iter().filter(|c| c.buff_id == id).collect();
        assert!(mine.len() >= 5, "buff {id} has only {} cells to compare", mine.len());
        assert!(
            mine.iter().any(|c| c.theirs > 0.0),
            "buff {id}: the EI export reports no uptime at all, so this proves nothing"
        );
        for c in mine {
            assert!(
                (c.ours - c.theirs).abs() <= DURATION_TOLERANCE_PP,
                "buff {id} on {}: ours {} vs EI {}",
                c.account,
                c.ours,
                c.theirs
            );
        }
    }
}

/// The KEY SET, not just the values: every (player, id) Elite Insights
/// reports with real uptime must exist on our side too. A pass that
/// produced correct numbers for the keys it emitted while silently dropping
/// whole players would pass the value check above.
#[test]
fn the_key_set_matches_the_ei_export() {
    let Some((ours, accounts, golden)) = calibration() else { return };
    let cells = cells(&ours, &accounts, &golden);
    assert!(cells.len() > 200, "the comparison must not be vacuous, got {} cells", cells.len());
    let by_account: BTreeMap<&str, u64> =
        accounts.iter().map(|(&addr, acc)| (acc.as_str(), addr)).collect();

    // The join must be total over the SQUAD: `cells` silently `continue`s
    // past any EI player whose account key we do not carry, so an
    // account-naming drift on either side would shrink the comparison
    // without failing anything. Pin it.
    //
    // The scope is squad-only on both sides. Elite Insights' `players`
    // array on a WvW capture also carries the enemy players it saw, flagged
    // `notInSquad: true` and named `"Non Squad Player <n>"` rather than by
    // account -- 4 of the 48 entries here. Those legitimately do not join:
    // `self_effects` is squad-scoped by construction (`build_with_registry`
    // drops any event whose owner is not a squad addr) and the enemy side is
    // `target_conditions`' job. So the invariant is that every IN-SQUAD EI
    // player joins, and separately that the excluded ones are excluded for
    // that reason and no other.
    let players = golden["players"].as_array().expect("players");
    let (squad, non_squad): (Vec<&Value>, Vec<&Value>) =
        players.iter().partition(|p| !p["notInSquad"].as_bool().unwrap_or(false));
    assert!(!squad.is_empty(), "the export must carry squad players");
    let unjoined: Vec<&str> = squad
        .iter()
        .map(|p| account_key(p["account"].as_str().expect("account")))
        .filter(|acc| !by_account.contains_key(acc))
        .collect();
    assert!(
        unjoined.is_empty(),
        "{} of {} in-squad Elite Insights players did not join to a squad \
         representative: {}",
        unjoined.len(),
        squad.len(),
        unjoined.join(", ")
    );
    // ...and nothing joins that should not have: the out-of-squad entries
    // are exactly the ones we skip, so the skip count is fully explained.
    for p in &non_squad {
        let acc = account_key(p["account"].as_str().expect("account"));
        assert!(
            !by_account.contains_key(acc),
            "{acc} is flagged notInSquad by EI but joined a squad representative"
        );
    }
    let mut missing: Vec<String> = Vec::new();
    for c in &cells {
        if c.theirs <= 0.0 {
            continue;
        }
        let addr = by_account[c.account.as_str()];
        if !ours.states.contains_key(&(addr, c.buff_id)) {
            missing.push(format!("{} buff {} (EI uptime {})", c.account, c.buff_id, c.theirs));
        }
    }
    assert!(missing.is_empty(), "{} EI keys have no timeline:\n{}", missing.len(), missing.join("\n"));
}
