//! MSTREAM measurement scaffold: the two candidate implementations of the
//! SDK-facing `to_ei_json` (which must hand back a materialized
//! `serde_json::Value`, because napi/pythonize walk a tree).
//!
//! * `to_value` — serialize the shared `ei_doc` into `serde_json::Value`
//!   directly (what shipped).
//! * `string` — `write_ei_json` into a `String`, then `from_str` it back.
//!
//! Run: `cargo run --release -p axilog-ei --example mstream_sdk_options -- <log> <to_value|string>`
//! and wrap it in `/usr/bin/time -v` for peak RSS.

use axilog_ei::EiInputs;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: <log> <to_value|string>");
    let mode = args.next().unwrap_or_else(|| "to_value".to_string());

    let bytes = std::fs::read(&path).expect("read log");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
    let report = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-bench", None, None, true, true, true, None,
    );
    let minions = axilog_core::analysis::minions::build(&raw, &enc);
    let health_percents = axilog_core::analysis::health::ei_health_percents(&raw, &enc);
    let enemies: std::collections::BTreeSet<u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
    let rep: std::collections::BTreeMap<u64, u64> =
        enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id))).collect();
    let enemy_dist = axilog_core::analysis::skill_damage::build_enemy_dist(&raw, &enemies, &rep);
    let reg = axilog_core::analysis::damage::InstidRegistry::build(&raw);
    let enemy_series =
        axilog_core::analysis::timeseries::build_enemy_series(&enc, &raw, &reg, &enemies, &rep);
    let dist_outcomes = axilog_core::analysis::dist_outcomes::build(&raw, &enc);
    let healing_detail = axilog_core::analysis::healing_detail::build(&raw, &enc);
    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, "0.0.0-bench", None,
        &axilog_schema::v1::Passes {
            minions: Some(&minions),
            health_percents: Some(&health_percents),
            enemy_dist: Some(&enemy_dist),
            enemy_series: Some(&enemy_series),
            dist_outcomes: Some(&dist_outcomes),
            healing_detail: healing_detail.as_ref(),
            healing_series: healing_detail.as_ref(),
            ..Default::default()
        },
    );
    let boon_states = axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons);
    let target_conditions = axilog_core::analysis::target_conditions::build(&raw, &enc);

    let inputs = EiInputs {
        activity: &activity,
        replay: None,
        modifiers: None,
        boon_states: Some(&boon_states),
        target_conditions: Some(&target_conditions),
    };

    let t0 = std::time::Instant::now();
    let v: serde_json::Value = match mode.as_str() {
        "to_value" => axilog_ei::to_ei_json(&report_v1, &report, &inputs),
        "string" => {
            let mut buf: Vec<u8> = Vec::new();
            axilog_ei::write_ei_json(&report_v1, &report, &inputs, &mut buf).expect("stream");
            serde_json::from_slice(&buf).expect("parse back")
        }
        other => panic!("unknown mode {other}"),
    };
    // Touch the tree so nothing above is optimized away.
    let n = v.as_object().expect("root object").len();
    eprintln!("mode={mode} root_keys={n} elapsed={:?}", t0.elapsed());
}
