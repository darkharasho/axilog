//! M16 Task 1: damage-modifier engine calibration against a real Elite
//! Insights export, using the ONE definition Task 1 ships
//! (`analysis::damage_mods::catalog::MOVING_BONUS`, GW2EI `Mod_MovingBonus
//! = 10` -> `damageModMap.d10`).
//!
//! This is the engine's smoke proof: `Moving Bonus` exercises the
//! eligibility rule (connected strike hits, actor-only), the constant
//! `DamageLogDamageModifier` gain (`5/105`), the `compare_type` denominator
//! and the 3-decimal rounding, with no buff state in the way -- so if its
//! four fields match a real export, the framework's plumbing is right.
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
use axilog_core::analysis::damage_mods::{evaluate_catalog, DamageModifierStat};
use axilog_core::evtc::decode_raw;
use axilog_core::model::resolve;
use std::collections::HashMap;

mod common;

/// GW2EI `Mod_MovingBonus` -- `DamageModifierIDs.cs:23`.
const MOVING_BONUS_ID: i32 = 10;

/// **Every field is checked EXACT.** Measured on the reference capture:
/// all 44 joined accounts match GW2EI's `hitCount`/`totalHitCount`/
/// `damageGain`/`totalDamage` to the last emitted digit -- so there is no
/// tolerance to spend, and any future drift should fail loudly rather than
/// hide inside a percentage band. `damageGain` gets a float epsilon only
/// because both sides are 3-decimal-rounded doubles.
///
/// (Getting here required one fix the calibration itself surfaced: a real
/// damage row with `dst_agent == 0` but a live `dst_instid`, which this
/// project's addr-keyed foe set silently dropped and GW2EI's instid-keyed
/// parser did not. See `analysis::damage_mods`'s zero-addr repair.)
const GAIN_EPSILON: f64 = 5e-4;

/// The `d10` row of a golden player's `damageModifiers` array, if present.
fn golden_moving_bonus(p: &serde_json::Value) -> Option<&serde_json::Value> {
    p.get("damageModifiers")?
        .as_array()?
        .iter()
        .find(|m| m["id"].as_i64() == Some(MOVING_BONUS_ID as i64))?
        .get("damageModifiers")?
        .as_array()?
        .first()
}

#[test]
fn moving_bonus_matches_the_local_reference_export_when_available() {
    let zevtc = common::local_fixture("wvw-postrework.zevtc");
    let ei_json = common::local_fixture("wvw-postrework.ei.json");
    let Some(bytes) = std::fs::read(&zevtc).ok() else {
        println!("skip: {zevtc} absent (Moving Bonus calibration)");
        return;
    };
    let Some(golden_s) = std::fs::read_to_string(&ei_json).ok() else {
        println!("skip: {ei_json} absent (Moving Bonus calibration)");
        return;
    };
    let golden: serde_json::Value =
        serde_json::from_str(&golden_s).unwrap_or_else(|e| panic!("parse {ei_json}: {e}"));

    let raw = decode_raw(&bytes).expect("decode postrework fixture");
    let enc = resolve(&raw);
    let registry = InstidRegistry::build(&raw);
    let stats = evaluate_catalog(&raw, &registry, &enc);

    // Sanity: the reference export must actually carry the descriptor we
    // calibrate against, or this test is vacuous.
    assert!(
        golden["damageModMap"].get("d10").is_some(),
        "reference export has no damageModMap.d10 -- calibration target missing"
    );

    let mut golden_by_account: HashMap<String, &serde_json::Value> = HashMap::new();
    for p in golden["players"].as_array().expect("players array") {
        if let Some(a) = p["account"].as_str() {
            golden_by_account.insert(common::account_key(a).to_string(), p);
        }
    }

    let mut joined = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for p in &enc.players {
        let key = common::account_key(&p.account).to_string();
        let Some(golden_p) = golden_by_account.get(&key) else { continue };
        let Some(g) = golden_moving_bonus(golden_p) else { continue };
        let ours: DamageModifierStat = stats
            .get(&(p.agent_addr, MOVING_BONUS_ID))
            .copied()
            .unwrap_or_default();
        joined += 1;

        let g_hit = g["hitCount"].as_i64().unwrap_or(0);
        let g_total_hit = g["totalHitCount"].as_i64().unwrap_or(0);
        let g_gain = g["damageGain"].as_f64().unwrap_or(0.0);
        let g_total = g["totalDamage"].as_f64().unwrap_or(0.0);

        println!(
            "d10 {key}: hits {}/{} (golden {g_hit}/{g_total_hit}), gain {:.3} (golden {g_gain:.3}), \
             total {} (golden {g_total:.0})",
            ours.hit_count, ours.total_hit_count, ours.damage_gain, ours.total_damage
        );

        if ours.hit_count as i64 != g_hit {
            mismatches.push(format!("{key} hitCount: ours={} golden={g_hit}", ours.hit_count));
        }
        if ours.total_hit_count as i64 != g_total_hit {
            mismatches.push(format!(
                "{key} totalHitCount: ours={} golden={g_total_hit}",
                ours.total_hit_count
            ));
        }
        if (ours.damage_gain - g_gain).abs() > GAIN_EPSILON {
            mismatches.push(format!(
                "{key} damageGain: ours={:.3} golden={g_gain:.3}",
                ours.damage_gain
            ));
        }
        if ours.total_damage as f64 != g_total {
            mismatches.push(format!(
                "{key} totalDamage: ours={} golden={g_total:.0}",
                ours.total_damage
            ));
        }

        // Structural invariants that must hold regardless of tolerance.
        assert!(
            ours.hit_count <= ours.total_hit_count,
            "{key}: hitCount must not exceed totalHitCount"
        );
        // A 5% multiplier gain can never exceed 5/105 of the denominator.
        assert!(
            ours.damage_gain <= ours.total_damage as f64 * (5.0 / 105.0) + 1.0,
            "{key}: damageGain exceeds the theoretical 5/105 ceiling"
        );
    }

    // Guard against the join silently degrading: the reference capture has
    // 44 joinable accounts carrying a `d10` row.
    assert!(joined >= 44, "only {joined} account(s) joined -- expected at least 44");
    println!("moving_bonus calibration: {joined} account(s) joined");
    assert!(mismatches.is_empty(), "Moving Bonus mismatches:\n{}", mismatches.join("\n"));
}
