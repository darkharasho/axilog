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

/// Shared decode -> resolve -> analyze -> build_report pipeline (identical
/// to `axilog-cli`'s `Cmd::Parse` handler and the Node SDK's
/// `build_report_from_bytes`) over an already-read byte buffer.
/// `want_replay` mirrors the `replay` keyword arg (M9, Task 2);
/// `want_skill_damage` mirrors the `skill_damage` keyword arg (M12, Task
/// 1); `want_missiles` mirrors the `missiles` keyword arg (final-review
/// fix wave) -- all defaulted to `false` by every existing call site.
#[allow(clippy::too_many_arguments)]
fn build_report_from_bytes(
    bytes: &[u8],
    want_replay: bool,
    want_skill_damage: bool,
    want_timeseries: bool,
    want_missiles: bool,
    want_rotation: bool,
    want_modifiers: bool,
) -> PyResult<axilog_schema::Report> {
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
    Ok(axilog_schema::build_report(
        &enc, &metrics, env!("CARGO_PKG_VERSION"), replay.as_ref(), missiles.as_ref(),
        want_skill_damage, want_timeseries, want_rotation, damage_mods.as_ref(),
    ))
}

/// `build_report_and_activity_from_bytes`'s return tuple. Named because it
/// grew a fourth member in M16 (and a fifth in MEIGAP) and
/// `clippy::type_complexity` is right that
/// the inline form had stopped being readable.
type EiPipelineOutputs = (
    axilog_schema::Report,
    Vec<axilog_core::analysis::replay::ActivityIntervals>,
    Option<axilog_core::analysis::ei_replay::EiReplay>,
    Option<axilog_core::analysis::damage_mods::DamageModifierResults>,
    Option<axilog_core::analysis::buffs::BoonStates>,
    Option<std::collections::BTreeMap<u64, axilog_core::analysis::timeseries::EnemySeries>>,
    Option<std::collections::BTreeMap<u64, Vec<axilog_core::analysis::skill_damage::SkillEntry>>>,
    Option<axilog_core::analysis::target_conditions::TargetConditionStates>,
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
    // MEIGAP Task 1b: GW2EI-shape boon stack timelines
    // (`buffUptimes[].states`/`.statesPerSource`), gated on the timeseries
    // flag -- the same setting GW2EI itself gates those two arrays behind
    // (`RawFormatTimelineArrays`). See
    // `axilog_core::analysis::buffs::states`'s module doc.
    let boon_states = want_timeseries
        .then(|| axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons));
    // MEIGAP Task 2b/2c/2d: the three `targets[]` mirrors. `enemy_series`
    // and `target_conditions` ride the timeseries flag (GW2EI's own
    // `RawFormatTimelineArrays` gate on `targets[].damage1S` at
    // `JsonActorBuilder.cs:102` and on `statesPerSource` at
    // `JsonBuffsUptimeBuilder.cs:52`); `enemy_dist` rides the skill-damage
    // flag, the one that already gates every other per-skill block. All
    // three are standalone passes -- `analyze()` above does not compute
    // them, so an unflagged call pays nothing.
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
    let enemy_series = enemy_sets.as_ref().filter(|_| want_timeseries).map(|(en, rep)| {
        axilog_core::analysis::timeseries::build_enemy_series(
            &enc,
            &raw,
            &axilog_core::analysis::damage::InstidRegistry::build(&raw),
            en,
            rep,
        )
    });
    let enemy_dist = enemy_sets
        .as_ref()
        .filter(|_| want_skill_damage)
        .map(|(en, rep)| axilog_core::analysis::skill_damage::build_enemy_dist(&raw, en, rep));
    let target_conditions =
        want_timeseries.then(|| axilog_core::analysis::target_conditions::build(&raw, &enc));
    Ok((
        report,
        activity,
        ei_replay,
        damage_mods,
        boon_states,
        enemy_series,
        enemy_dist,
        target_conditions,
    ))
}

fn report_to_value(report: &axilog_schema::Report) -> PyResult<Value> {
    serde_json::to_value(report).map_err(value_err)
}

fn value_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    Ok(pythonize(py, value).map_err(value_err)?.unbind())
}

/// Parses a `.evtc`/`.zevtc` file at `path` and returns the native
/// `Report` as a plain Python dict (see module docs for the field-name
/// behavior). `replay=True` (M9, Task 2) opts into embedding the native
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
    let report = build_report_from_bytes(&bytes, replay, skill_damage, timeseries, missiles, rotation, modifiers)?;
    value_to_py(py, &report_to_value(&report)?)
}

/// Parses an already-read `.evtc`/`.zevtc` buffer (`bytes`/`bytearray`)
/// and returns the native `Report` as a plain Python dict. `replay=True`
/// (M9, Task 2) opts into embedding the native combat-replay block;
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
    let report = build_report_from_bytes(data, replay, skill_damage, timeseries, missiles, rotation, modifiers)?;
    value_to_py(py, &report_to_value(&report)?)
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
    let (report, activity, ei_replay, damage_mods, boon_states, enemy_series, enemy_dist, target_conditions) =
        build_report_and_activity_from_bytes(&bytes, replay, skill_damage, timeseries, missiles, rotation, modifiers)?;
    let ei = axilog_ei::to_ei_json(
        &report,
        &axilog_ei::EiInputs {
            activity: &activity,
            replay: ei_replay.as_ref(),
            modifiers: damage_mods.as_ref(),
            boon_states: boon_states.as_ref(),
            enemy_series: enemy_series.as_ref(),
            enemy_dist: enemy_dist.as_ref(),
            target_conditions: target_conditions.as_ref(),
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
