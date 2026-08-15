//! PyO3 Python module bindings for axilog's Rust parsing core (M6 Task 1).
//!
//! Every export here is a thin wrapper around the same pipeline the CLI
//! (`crates/axilog-cli/src/main.rs`) and the Node SDK
//! (`crates/axilog-node/src/lib.rs`) drive: `evtc::decode_raw` ->
//! `model::resolve` -> `analysis::analyze` -> `axilog_schema::build_report`,
//! optionally through `axilog_ei::to_ei_json`. No Rust panic is allowed to
//! cross the FFI boundary: every fallible step (`std::io::Error`,
//! `axilog_core::evtc::EvtcError`, `serde_json`/`pythonize` conversion
//! errors) is mapped to a `PyErr` carrying the original Rust message via
//! `Display`/`ToString`, never `.unwrap()`ed or `.expect()`ed.
//!
//! Error mapping (per the M6 brief): failures reading/writing a path on
//! disk (`std::io::Error`) become Python `OSError`; failures decoding or
//! parsing evtc bytes (`axilog_core::evtc::EvtcError`) or serializing the
//! result (`serde_json::Error`/`pythonize::PythonizeError`) become Python
//! `ValueError`. Both carry the Rust error's `Display` text verbatim.
//!
//! Return values go through `serde_json::to_value` (the same
//! `Report`/`to_ei_json` output the Node SDK returns) and then
//! `pythonize::pythonize`, which walks a `serde_json::Value` into native
//! Python `dict`/`list`/`str`/`int`/`float`/`bool`/`None` -- so the
//! returned dict's keys are exactly the serde field names already on
//! `Report`/`to_ei_json` (snake_case, e.g. `schema_version`), never
//! re-cased.
#![deny(clippy::all)]

use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pythonize::pythonize;
use serde_json::Value;

/// Rust `std::io::Error` (file read/write failures) -> Python `OSError`,
/// carrying the original message.
fn io_err(e: std::io::Error) -> PyErr {
    PyOSError::new_err(e.to_string())
}

/// Any decode/parse/serialize failure (`axilog_core::evtc::EvtcError`,
/// `serde_json::Error`, `pythonize::PythonizeError`) -> Python
/// `ValueError`, carrying the Rust `Display` message.
fn value_err(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// Shared decode -> resolve -> analyze -> build_report -> build_report_v1
/// pipeline (identical up through `build_report` to `axilog-cli`'s
/// `Cmd::Parse` handler and the Node SDK's `build_report_v1_from_bytes`)
/// over an already-read byte buffer, returning the native 1.0 container
/// (Task 12: `parse_file`/`parse_bytes` emit the 1.0 document, mirroring
/// the CLI's `--format json`; `parse_file_ei` is untouched and keeps
/// consuming the legacy `Report` via `build_report_and_activity_from_bytes`
/// below). `want_replay` mirrors the `replay` keyword arg (M9, Task 2);
/// `want_skill_damage` mirrors the `skill_damage` keyword arg (M12, Task
/// 1); `want_missiles` mirrors the `missiles` keyword arg (final-review
/// fix wave) -- all defaulted to `false` by every existing call site.
/// `generated_from` is the origin file NAME (never a full path -- paths
/// are environment-specific and routinely contain a user name, which the
/// PII policy scrubs); `parse_bytes` has no file name to offer and passes
/// `None`.
#[allow(clippy::too_many_arguments)]
fn build_report_v1_from_bytes(
    bytes: &[u8],
    want_replay: bool,
    want_skill_damage: bool,
    want_timeseries: bool,
    want_missiles: bool,
    want_rotation: bool,
    want_modifiers: bool,
    generated_from: Option<&str>,
) -> PyResult<axilog_schema::v1::ReportV1> {
    let raw = axilog_core::evtc::decode_raw(bytes).map_err(value_err)?;
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let replay = want_replay.then(|| {
        axilog_core::analysis::replay::build_replay(
            &raw,
            &enc,
            axilog_core::analysis::replay::DEFAULT_POLL_MS,
        )
    });
    let missiles = want_missiles
        .then(|| axilog_core::analysis::missiles::build_missiles(&raw, &enc));
    // Native path: whole-fight only -- the per-target split has no native
    // counterpart (see `axilog_ei::EiInputs::modifiers`).
    let damage_mods = want_modifiers.then(|| {
        axilog_core::analysis::damage_mods::evaluate_catalog_full(
            &raw, &axilog_core::analysis::damage::InstidRegistry::build(&raw), &enc, false,
        )
    });
    // Side-channel absorption Task 6: these two passes were previously run
    // only on the ei-json path, so the NATIVE path emitted no `minions`
    // block and no `healthPercents` even when the caller asked for the
    // gates that produce them. They are native blocks now, so they run
    // here on the same options that gate them everywhere else.
    let minion_rollups =
        want_skill_damage.then(|| axilog_core::analysis::minions::build(&raw, &enc));
    let health_percents =
        want_timeseries.then(|| axilog_core::analysis::health::ei_health_percents(&raw, &enc));
    // Task 7, same story: enemy per-skill damage now lands on the native
    // `damage` block, so the native path has to run the pass too.
    let enemy_dist = want_skill_damage.then(|| {
        let enemies: std::collections::BTreeSet<u64> =
            enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
        let rep: std::collections::BTreeMap<u64, u64> = enc
            .enemies
            .iter()
            .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
            .collect();
        axilog_core::analysis::skill_damage::build_enemy_dist(&raw, &enemies, &rep)
    });
    let report = axilog_schema::build_report(
        &enc, &metrics, env!("CARGO_PKG_VERSION"), replay.as_ref(), missiles.as_ref(),
        want_skill_damage, want_timeseries, want_rotation, damage_mods.as_ref(),
    );
    Ok(axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &report,
        env!("CARGO_PKG_VERSION"),
        generated_from,
        &axilog_schema::v1::Passes {
            damage_mods: damage_mods.as_ref(),
            minions: minion_rollups.as_ref(),
            health_percents: health_percents.as_ref(),
            enemy_dist: enemy_dist.as_ref(),
        },
    ))
}

/// `build_report_and_activity_from_bytes`'s return tuple. Named because it
/// grew a fourth member in M16 (and a fifth in MEIGAP) and
/// `clippy::type_complexity` is right that
/// the inline form had stopped being readable.
type EiPipelineOutputs = (
    axilog_schema::Report,
    axilog_schema::v1::ReportV1,
    Vec<axilog_core::analysis::replay::ActivityIntervals>,
    Option<axilog_core::analysis::ei_replay::EiReplay>,
    Option<axilog_core::analysis::damage_mods::DamageModifierResults>,
    Option<axilog_core::analysis::buffs::BoonStates>,
    Option<std::collections::BTreeMap<u64, axilog_core::analysis::timeseries::EnemySeries>>,
    Option<axilog_core::analysis::target_conditions::TargetConditionStates>,
    Option<axilog_core::analysis::healing_detail::HealingDetail>,
    Option<std::collections::BTreeMap<u64, axilog_core::analysis::dist_outcomes::DistOutcomes>>,
);

/// Same decode -> resolve -> analyze pipeline as `build_report_from_bytes`,
/// but additionally returns the M11 Task 3 activity intervals
/// (`axilog_core::analysis::replay::build_activity_intervals`) the ei-json
/// adapter needs for `combatReplayData`/`activeTimes` -- computed
/// unconditionally (cheap, unlike `--replay`'s position track), independent
/// of `want_replay`.
///
/// `want_skill_damage`/`want_timeseries` (final-review fix wave) are
/// threaded into the `build_report` call below so `parse_file_ei`'s
/// `skill_damage=True`/`timeseries=True` keyword args can actually surface
/// `totalDamageDist`/`damage1S`/etc in the returned ei-json (see
/// `axilog_ei::to_ei_json`, which reads those fields straight off
/// `PlayerOut::skill_damage`/`PlayerOut::per_second` -- previously always
/// `None` here regardless of what the caller asked for). `want_missiles`
/// is threaded the same way for symmetry with `parse_file`/`parse_bytes`,
/// even though `to_ei_json` does not currently read `Report::missiles`.
#[allow(clippy::too_many_arguments)]
fn build_report_and_activity_from_bytes(
    bytes: &[u8],
    want_replay: bool,
    want_skill_damage: bool,
    want_timeseries: bool,
    want_missiles: bool,
    want_rotation: bool,
    want_modifiers: bool,
) -> PyResult<EiPipelineOutputs> {
    let raw = axilog_core::evtc::decode_raw(bytes).map_err(value_err)?;
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let replay = want_replay.then(|| {
        axilog_core::analysis::replay::build_replay(
            &raw,
            &enc,
            axilog_core::analysis::replay::DEFAULT_POLL_MS,
        )
    });
    let missiles = want_missiles
        .then(|| axilog_core::analysis::missiles::build_missiles(&raw, &enc));
    let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
    // M15 Task 3: `replay=True` now DOES affect the ei-json -- it adds
    // `combatReplayData.{positions, orientations, dc, iconURL}` and the
    // top-level `combatReplayMetaData` (see `axilog_ei::to_ei_json`).
    let ei_replay = want_replay
        .then(|| axilog_core::analysis::ei_replay::build_ei_replay_auto(&raw, &enc));
    // ei-json path: WITH the per-target split, which is the one shape
    // `damageModifiersTarget`/`incomingDamageModifiersTarget` need.
    let damage_mods = want_modifiers.then(|| {
        axilog_core::analysis::damage_mods::evaluate_catalog_full(
            &raw, &axilog_core::analysis::damage::InstidRegistry::build(&raw), &enc, true,
        )
    });
    let report = axilog_schema::build_report(
        &enc, &metrics, env!("CARGO_PKG_VERSION"), replay.as_ref(), missiles.as_ref(),
        want_skill_damage, want_timeseries, want_rotation, damage_mods.as_ref(),
    );
    // Side-channel absorption Task 3: the transitional `ei_doc`/
    // `to_ei_json` signature now also needs the 1.0 `ReportV1` alongside
    // `report`. `parse_file_ei`/`parse_buffer_ei` have no file name to
    // offer (unlike `parse_file`'s own `build_report_v1_from_bytes`), so
    // `generated_from` stays `None`, matching this function's existing
    // convention.
    // Task 6: hoisted above the `ReportV1` build, which now consumes them
    // as native blocks (`minions`, and `healthPercents` on `series`).
    // MEIGAP Task 3b rides `--skill-damage`; MEIGAP2 row 2 rides
    // `--timeseries`, GW2EI's own `RawFormatTimelineArrays` gate.
    let minion_rollups =
        want_skill_damage.then(|| axilog_core::analysis::minions::build(&raw, &enc));
    let health_percents =
        want_timeseries.then(|| axilog_core::analysis::health::ei_health_percents(&raw, &enc));
    // MEIGAP Task 2b/2c/2d: the three `targets[]` mirrors. `enemy_series`
    // and `target_conditions` ride the timeseries flag (GW2EI's own
    // `RawFormatTimelineArrays` gate on `targets[].damage1S` at
    // `JsonActorBuilder.cs:63` and on `statesPerSource` at
    // `JsonBuffsUptimeBuilder.cs:52`); `enemy_dist` rides the skill-damage
    // flag, the one that already gates every other per-skill block. All
    // three are standalone passes -- `analyze()` above does not compute
    // them, so an unflagged call pays nothing.
    //
    // Task 7 hoisted `enemy_sets`/`enemy_dist` above the `ReportV1` build,
    // which now consumes the latter as enemy rows on the `damage` block.
    let enemy_sets = (want_timeseries || want_skill_damage).then(|| {
        let enemies: std::collections::BTreeSet<u64> =
            enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
        let enemy_addr_to_rep: std::collections::BTreeMap<u64, u64> = enc
            .enemies
            .iter()
            .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
            .collect();
        (enemies, enemy_addr_to_rep)
    });
    let enemy_dist = enemy_sets
        .as_ref()
        .filter(|_| want_skill_damage)
        .map(|(en, rep)| axilog_core::analysis::skill_damage::build_enemy_dist(&raw, en, rep));
    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, env!("CARGO_PKG_VERSION"), None,
        &axilog_schema::v1::Passes {
            damage_mods: damage_mods.as_ref(),
            minions: minion_rollups.as_ref(),
            health_percents: health_percents.as_ref(),
            enemy_dist: enemy_dist.as_ref(),
        },
    );
    // MEIGAP Task 1b: GW2EI-shape boon stack timelines
    // (`buffUptimes[].states`/`.statesPerSource`), gated on the timeseries
    // flag -- the same setting GW2EI itself gates those two arrays behind
    // (`RawFormatTimelineArrays`). See
    // `axilog_core::analysis::buffs::states`'s module doc.
    let boon_states = want_timeseries
        .then(|| axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons));
    let enemy_series = enemy_sets.as_ref().filter(|_| want_timeseries).map(|(en, rep)| {
        axilog_core::analysis::timeseries::build_enemy_series(
            &enc,
            &raw,
            &axilog_core::analysis::damage::InstidRegistry::build(&raw),
            en,
            rep,
        )
    });
    let target_conditions =
        want_timeseries.then(|| axilog_core::analysis::target_conditions::build(&raw, &enc));
    // MEIGAP Task 3a/3b. Every healing-detail family is flag-gated in the
    // adapter (`healing1S` on timeseries; the ally matrices and the two
    // `*Dist` arrays on skill-damage -- see `EiInputs::healing_dist`), so
    // the pass only runs when at least one of them will be serialized. It
    // self-gates to `None` on a log with no healing extension.
    let healing_detail = (want_skill_damage || want_timeseries)
        .then(|| axilog_core::analysis::healing_detail::build(&raw, &enc))
        .flatten();
    // MEIGAP2 row 1 -- same gate the CLI uses (`--skill-damage`, the gate
    // on the distributions these columns annotate).
    let dist_outcomes =
        want_skill_damage.then(|| axilog_core::analysis::dist_outcomes::build(&raw, &enc));
    Ok((
        report,
        report_v1,
        activity,
        ei_replay,
        damage_mods,
        boon_states,
        enemy_series,
        target_conditions,
        healing_detail,
        dist_outcomes,
    ))
}

fn report_v1_to_value(report: &axilog_schema::v1::ReportV1) -> PyResult<Value> {
    serde_json::to_value(report).map_err(value_err)
}

fn value_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    Ok(pythonize(py, value).map_err(value_err)?.unbind())
}

/// Parses a `.evtc`/`.zevtc` file at `path` and returns the native 1.0
/// container (Task 12: `axilog_schema::v1::ReportV1`) as a plain Python
/// dict (see module docs for the field-name behavior). `path`'s file name
/// (never the full path) is threaded into the document's `generated_from`.
/// `replay=True` (M9, Task 2) opts into embedding the native
/// combat-replay block; `skill_damage=True` (M12, Task 1) opts into
/// embedding the native per-skill damage distribution block (measured
/// +249% JSON size on the committed fixture when always-on, see
/// `axilog_schema::Report::players`'s `PlayerOut::skill_damage` doc
/// comment -- hence opt-in). `timeseries=True` (M12, Task 2) opts into
/// embedding the native per-player per-second series block AND the
/// per-enemy `dps_targets` summary (measured +147.7%/+36.4% JSON size
/// respectively when always-on, see `PlayerOut::per_second`/`PlayerOut::
/// dps_targets`'s doc comments -- `dps_targets` is NOT small on a real
/// WvW log with many enemies, so both stay behind this one flag).
/// `missiles=True` (final-review fix wave) opts into embedding the native
/// top-level missile analytics block (`Report.missiles`), mirroring the
/// CLI's `--missiles` flag. `rotation=True` (M14, Task 1) opts into
/// embedding the native per-player rotation (cast tracking) block
/// (measured +66.9% JSON size on the committed fixture when always-on, see
/// `PlayerOut::rotation`'s doc comment). All five default to `False` for
/// back-compat with every existing positional-only call site.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (path, replay=false, skill_damage=false, timeseries=false, missiles=false, rotation=false, modifiers=false))]
fn parse_file(
    py: Python<'_>, path: &str, replay: bool, skill_damage: bool, timeseries: bool, missiles: bool,
    rotation: bool,
    modifiers: bool,
) -> PyResult<Py<PyAny>> {
    let bytes = std::fs::read(path).map_err(io_err)?;
    let generated_from = std::path::Path::new(path).file_name().and_then(|s| s.to_str());
    let report = build_report_v1_from_bytes(
        &bytes, replay, skill_damage, timeseries, missiles, rotation, modifiers, generated_from,
    )?;
    value_to_py(py, &report_v1_to_value(&report)?)
}

/// Parses an already-read `.evtc`/`.zevtc` buffer (`bytes`/`bytearray`)
/// and returns the native 1.0 container (Task 12:
/// `axilog_schema::v1::ReportV1`) as a plain Python dict. A buffer has no
/// file name to offer, so `generated_from` is always absent here.
/// `replay=True` (M9, Task 2) opts into embedding the native combat-replay block;
/// `skill_damage=True` (M12, Task 1) opts into embedding the native
/// per-skill damage distribution block; `timeseries=True` (M12, Task 2)
/// opts into embedding the native per-player per-second series block;
/// `missiles=True` (final-review fix wave) opts into embedding the native
/// top-level missile analytics block.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (data, replay=false, skill_damage=false, timeseries=false, missiles=false, rotation=false, modifiers=false))]
fn parse_bytes(
    py: Python<'_>, data: &[u8], replay: bool, skill_damage: bool, timeseries: bool, missiles: bool,
    rotation: bool,
    modifiers: bool,
) -> PyResult<Py<PyAny>> {
    let report = build_report_v1_from_bytes(
        data, replay, skill_damage, timeseries, missiles, rotation, modifiers, None,
    )?;
    value_to_py(py, &report_v1_to_value(&report)?)
}

/// Parses a `.evtc`/`.zevtc` file at `path` and returns the Elite
/// Insights-compatibility JSON (`axilog_ei::to_ei_json`) as a plain
/// Python dict. `skill_damage=True`/`timeseries=True` (final-review fix
/// wave) are what actually let `totalDamageDist`/`damage1S`/`dpsTargets`/
/// etc (M12, Task 3's ei-json mapping) surface in the returned JSON, since
/// `axilog_ei::to_ei_json` reads them straight off the native `Report`
/// this function builds internally; previously this function always built
/// that `Report` with both flags forced `False`, silently discarding any
/// M12 detail regardless of what a caller wanted. `replay=True` (M15,
/// Task 3) adds GW2EI's own combat-replay surface -- per-actor
/// `combatReplayData.{positions, orientations, dc, iconURL}` plus the
/// top-level `combatReplayMetaData` (it roughly triples the payload, hence
/// opt-in). `missiles=True` is accepted for signature parity with
/// `parse_file` but has no effect on the output -- EI's JSON shape has no
/// comparable field for it. All four default
/// to `False`, keeping the existing zero-arg call shape's behavior
/// unchanged.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (path, *, replay=false, skill_damage=false, timeseries=false, missiles=false, rotation=false, modifiers=false))]
fn parse_file_ei(
    py: Python<'_>, path: &str, replay: bool, skill_damage: bool, timeseries: bool, missiles: bool,
    rotation: bool,
    modifiers: bool,
) -> PyResult<Py<PyAny>> {
    let bytes = std::fs::read(path).map_err(io_err)?;
    let (report, report_v1, activity, ei_replay, damage_mods, boon_states, enemy_series,
         target_conditions, healing_detail, dist_outcomes) =
        build_report_and_activity_from_bytes(&bytes, replay, skill_damage, timeseries, missiles, rotation, modifiers)?;
    let ei = axilog_ei::to_ei_json(
        &report_v1,
        &report,
        &axilog_ei::EiInputs {
            activity: &activity,
            replay: ei_replay.as_ref(),
            modifiers: damage_mods.as_ref(),
            boon_states: boon_states.as_ref(),
            enemy_series: enemy_series.as_ref(),
            target_conditions: target_conditions.as_ref(),
            healing_detail: healing_detail.as_ref(),
            healing_series: timeseries,
            healing_dist: skill_damage,
            dist_outcomes: dist_outcomes.as_ref(),
        },
    );
    value_to_py(py, &ei)
}

/// Rewrites every player's character/account name in the `.zevtc` at
/// `in_path` to a deterministic `Anon<N>` placeholder and writes the
/// result to `out_path` (same transform as `axilog anonymize` / the core
/// `anonymize_raw_evtc` function -- reused here, not duplicated). Returns
/// the number of player agents rewritten.
#[pyfunction]
fn anonymize_file(in_path: &str, out_path: &str) -> PyResult<u32> {
    let bytes = std::fs::read(in_path).map_err(io_err)?;
    let mut data = axilog_core::evtc::inflate_zevtc(&bytes).map_err(value_err)?;
    let rewritten = axilog_core::evtc::anonymize_raw_evtc(&mut data).map_err(value_err)?;

    let entry_name = std::path::Path::new(out_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("log");
    let zipped = axilog_core::evtc::zip_deflate(&format!("{entry_name}.evtc"), &data);
    std::fs::write(out_path, zipped).map_err(io_err)?;

    Ok(rewritten as u32)
}

/// The `axilog` Python extension module.
#[pymodule]
fn axilog(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_file, m)?)?;
    m.add_function(wrap_pyfunction!(parse_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(parse_file_ei, m)?)?;
    m.add_function(wrap_pyfunction!(anonymize_file, m)?)?;
    Ok(())
}
