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
/// consuming the legacy `Report` via `build_report_and_ei_inputs_from_bytes`
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
    // counterpart on this path -- it is the expensive half, and only the
    // ei-json builder below asks for it (absorption Task 13 gave it a native
    // home on `blocks.damage_mods`, but not a reason to always pay for it).
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
    // Tasks 7 and 8, same story: enemy per-skill damage and the per-enemy
    // outgoing series now land on the native `damage` and `series` blocks,
    // so the native path has to run both passes too. The addr set and the
    // representative fold are shared, and built at most once.
    let enemy_sets = (want_skill_damage || want_timeseries).then(|| {
        let enemies: std::collections::BTreeSet<u64> =
            enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
        let rep: std::collections::BTreeMap<u64, u64> = enc
            .enemies
            .iter()
            .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
            .collect();
        (enemies, rep)
    });
    let enemy_dist = enemy_sets
        .as_ref()
        .filter(|_| want_skill_damage)
        .map(|(en, rep)| axilog_core::analysis::skill_damage::build_enemy_dist(&raw, en, rep));
    let enemy_series = enemy_sets.as_ref().filter(|_| want_timeseries).map(|(en, rep)| {
        axilog_core::analysis::timeseries::build_enemy_series(
            &enc,
            &raw,
            &axilog_core::analysis::damage::InstidRegistry::build(&raw),
            en,
            rep,
        )
    });
    // Task 10, the last of the same story: one pass, two families, two
    // flags -- so it runs on EITHER gate and each `Passes` field is
    // re-filtered to the flag that family actually rides.
    let healing_detail = (want_skill_damage || want_timeseries)
        .then(|| axilog_core::analysis::healing_detail::build(&raw, &enc))
        .flatten();
    // CC-strip-timelines Task 4: the per-player 1s CC/strip lanes on
    // `blocks.series.by_entity`. Gated on `--timeseries` because it is NOT
    // cheap: `build_from` derives an `InstidRegistry` (a full pass over
    // `raw.events`) and the pass itself makes several more scans on top of
    // that. Only the three address folds it also does are cheap.
    let entity_series = want_timeseries
        .then(|| axilog_core::analysis::entity_series::build_from(&enc, &raw, &metrics));
    let report = axilog_schema::build_report(
        &enc, &metrics, env!("CARGO_PKG_VERSION"), replay.as_ref(), missiles.as_ref(),
        want_skill_damage, want_timeseries, want_rotation, damage_mods.as_ref(),
    );
    // Task 9, same story again: the outcome columns are native now, so the
    // native path runs the pass on the gate that produces them.
    let dist_outcomes =
        want_skill_damage.then(|| axilog_core::analysis::dist_outcomes::build(&raw, &enc));
    // Task 11: ungated on purpose. `blocks.replay.by_entity` is the
    // always-on half of that block, so the native document carries
    // down/dead intervals whether or not positions were asked for.
    let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
    let replay_extras = axilog_core::analysis::replay_extras::build(&raw);
    // Task 12: the native path needs these too -- they feed
    // `blocks.boons`/`blocks.conditions`, not just the ei-json adapter.
    let boon_states = want_timeseries
        .then(|| axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons));
    let target_conditions =
        want_timeseries.then(|| axilog_core::analysis::target_conditions::build(&raw, &enc));
    let self_effects =
        want_timeseries.then(|| axilog_core::analysis::self_effects::build(&raw, &enc));
    // Always-on, like `activity` above: this pass emits uptime only, at
    // the cost `blocks.boons`' own always-on half already carries. Gating
    // it would empty axibridge's Special Buffs and Sigil/Relic sections on
    // every default parse.
    let squad_buffs = axilog_core::analysis::squad_buffs::build(&raw, &enc);
    let focus = axilog_core::analysis::focus::build(&enc, &raw);
    // The attributed detail behind `received_cc_count` and the `cc_taken`
    // lane -- which skill landed each incoming CC, from whom, and when.
    // Gated with the lane it decomposes (see `Passes::cc_taken_events`).
    let cc_taken_events =
        want_timeseries.then(|| axilog_core::analysis::cc::taken_events_for(&enc, &raw));
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
            enemy_series: enemy_series.as_ref(),
            dist_outcomes: dist_outcomes.as_ref(),
            healing_detail: healing_detail.as_ref().filter(|_| want_skill_damage),
            healing_series: healing_detail.as_ref().filter(|_| want_timeseries),
            entity_series: entity_series.as_ref(),
            activity: Some(&activity),
            replay_extras: Some(&replay_extras),
            boon_states: boon_states.as_ref(),
            target_conditions: target_conditions.as_ref(),
            self_effects: self_effects.as_ref(),
            squad_buffs: Some(&squad_buffs),
            focus: Some(&focus),
            cc_taken_events: cc_taken_events.as_deref(),
        },
    ))
}

/// `build_report_and_ei_inputs_from_bytes`'s return tuple. Named because it
/// grew a fourth member in M16 (and a fifth in MEIGAP) and
/// `clippy::type_complexity` is right that
/// the inline form had stopped being readable.
type EiPipelineOutputs = (
    axilog_schema::v1::ReportV1,
    Option<axilog_core::analysis::ei_replay::EiReplay>,
);

/// Same decode -> resolve -> analyze pipeline as `build_report_from_bytes`,
/// but additionally returns the one EI-SHAPE input the adapter still takes
/// -- the combat-replay position surface the native document deliberately
/// does not model (see `axilog_ei::EiReplayInput`). The M11 Task 3 activity intervals used to be among them;
/// side-channel absorption Task 11 moved them onto
/// `blocks.replay.by_entity`, so both builders now compute them and neither
/// hands them out.
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
fn build_report_and_ei_inputs_from_bytes(
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
    let replay_extras = axilog_core::analysis::replay_extras::build(&raw);
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
    // Task 10, the last of the same story: one pass, two families, two
    // flags -- so it runs on EITHER gate and each `Passes` field is
    // re-filtered to the flag that family actually rides.
    let healing_detail = (want_skill_damage || want_timeseries)
        .then(|| axilog_core::analysis::healing_detail::build(&raw, &enc))
        .flatten();
    // CC-strip-timelines Task 4: the per-player 1s CC/strip lanes on
    // `blocks.series.by_entity`. Gated on `--timeseries` because it is NOT
    // cheap: `build_from` derives an `InstidRegistry` (a full pass over
    // `raw.events`) and the pass itself makes several more scans on top of
    // that. Only the three address folds it also does are cheap.
    let entity_series = want_timeseries
        .then(|| axilog_core::analysis::entity_series::build_from(&enc, &raw, &metrics));
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
    // Tasks 7 and 8 hoisted `enemy_sets`/`enemy_dist`/`enemy_series` above
    // the `ReportV1` build, which now consumes both passes as enemy rows on
    // the `damage` and `series` blocks.
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
    // Task 8: hoisted above the `ReportV1` build for the same reason Task 7
    // hoisted `enemy_dist` -- the native `series` block consumes it now.
    let enemy_series = enemy_sets.as_ref().filter(|_| want_timeseries).map(|(en, rep)| {
        axilog_core::analysis::timeseries::build_enemy_series(
            &enc,
            &raw,
            &axilog_core::analysis::damage::InstidRegistry::build(&raw),
            en,
            rep,
        )
    });
    // MEIGAP2 row 1 -- same gate the CLI uses (`--skill-damage`, the gate
    // on the distributions these columns annotate). Computed BEFORE the
    // reprojection, not after: side-channel absorption Task 9 made it an
    // input to `blocks.damage` rather than a side channel handed to the
    // ei-json adapter, so it has to exist by the time that block is built.
    let dist_outcomes =
        want_skill_damage.then(|| axilog_core::analysis::dist_outcomes::build(&raw, &enc));
    // MEIGAP Task 1b / Task 2d: the two timeline passes, gated on the
    // timeseries flag -- the same setting GW2EI gates their arrays behind
    // (`RawFormatTimelineArrays`). Absorption Task 12 made them inputs to
    // `blocks.boons`/`blocks.conditions` rather than side channels handed
    // to the ei-json adapter, so they must exist before the reprojection.
    let boon_states = want_timeseries
        .then(|| axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons));
    let target_conditions =
        want_timeseries.then(|| axilog_core::analysis::target_conditions::build(&raw, &enc));
    let self_effects =
        want_timeseries.then(|| axilog_core::analysis::self_effects::build(&raw, &enc));
    // Always-on, like `activity` above: this pass emits uptime only, at
    // the cost `blocks.boons`' own always-on half already carries. Gating
    // it would empty axibridge's Special Buffs and Sigil/Relic sections on
    // every default parse.
    let squad_buffs = axilog_core::analysis::squad_buffs::build(&raw, &enc);
    let focus = axilog_core::analysis::focus::build(&enc, &raw);
    // The attributed detail behind `received_cc_count` and the `cc_taken`
    // lane -- which skill landed each incoming CC, from whom, and when.
    // Gated with the lane it decomposes (see `Passes::cc_taken_events`).
    let cc_taken_events =
        want_timeseries.then(|| axilog_core::analysis::cc::taken_events_for(&enc, &raw));
    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, env!("CARGO_PKG_VERSION"), None,
        &axilog_schema::v1::Passes {
            damage_mods: damage_mods.as_ref(),
            minions: minion_rollups.as_ref(),
            health_percents: health_percents.as_ref(),
            enemy_dist: enemy_dist.as_ref(),
            enemy_series: enemy_series.as_ref(),
            dist_outcomes: dist_outcomes.as_ref(),
            healing_detail: healing_detail.as_ref().filter(|_| want_skill_damage),
            healing_series: healing_detail.as_ref().filter(|_| want_timeseries),
            entity_series: entity_series.as_ref(),
            activity: Some(&activity),
            replay_extras: Some(&replay_extras),
            boon_states: boon_states.as_ref(),
            target_conditions: target_conditions.as_ref(),
            self_effects: self_effects.as_ref(),
            squad_buffs: Some(&squad_buffs),
            focus: Some(&focus),
            cc_taken_events: cc_taken_events.as_deref(),
        },
    );
    Ok((report_v1, ei_replay))
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
/// The six compute gates, resolved once from the keyword arguments.
///
/// `everything` is folded in HERE rather than at each gate's own read
/// site, so a pass added later cannot be left out of it by forgetting one
/// `|| everything` at one of three entry points -- which is exactly the
/// option-list drift `everything` exists to prevent.
#[derive(Clone, Copy)]
struct Gates {
    replay: bool,
    skill_damage: bool,
    timeseries: bool,
    missiles: bool,
    rotation: bool,
    modifiers: bool,
}

impl Gates {
    #[allow(clippy::too_many_arguments)]
    fn resolve(
        replay: bool,
        skill_damage: bool,
        timeseries: bool,
        missiles: bool,
        rotation: bool,
        modifiers: bool,
        everything: bool,
    ) -> Self {
        Gates {
            replay: replay || everything,
            skill_damage: skill_damage || everything,
            timeseries: timeseries || everything,
            missiles: missiles || everything,
            rotation: rotation || everything,
            modifiers: modifiers || everything,
        }
    }
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (path, replay=false, skill_damage=false, timeseries=false, missiles=false, rotation=false, modifiers=false, everything=false))]
fn parse_file(
    py: Python<'_>, path: &str, replay: bool, skill_damage: bool, timeseries: bool, missiles: bool,
    rotation: bool,
    modifiers: bool,
    everything: bool,
) -> PyResult<Py<PyAny>> {
    let g = Gates::resolve(replay, skill_damage, timeseries, missiles, rotation, modifiers, everything);
    let bytes = std::fs::read(path).map_err(io_err)?;
    let generated_from = std::path::Path::new(path).file_name().and_then(|s| s.to_str());
    let report = build_report_v1_from_bytes(
        &bytes, g.replay, g.skill_damage, g.timeseries, g.missiles, g.rotation, g.modifiers,
        generated_from,
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
#[pyo3(signature = (data, replay=false, skill_damage=false, timeseries=false, missiles=false, rotation=false, modifiers=false, everything=false))]
fn parse_bytes(
    py: Python<'_>, data: &[u8], replay: bool, skill_damage: bool, timeseries: bool, missiles: bool,
    rotation: bool,
    modifiers: bool,
    everything: bool,
) -> PyResult<Py<PyAny>> {
    let g = Gates::resolve(replay, skill_damage, timeseries, missiles, rotation, modifiers, everything);
    let report = build_report_v1_from_bytes(
        data, g.replay, g.skill_damage, g.timeseries, g.missiles, g.rotation, g.modifiers, None,
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
#[pyo3(signature = (path, *, replay=false, skill_damage=false, timeseries=false, missiles=false, rotation=false, modifiers=false, everything=false))]
fn parse_file_ei(
    py: Python<'_>, path: &str, replay: bool, skill_damage: bool, timeseries: bool, missiles: bool,
    rotation: bool,
    modifiers: bool,
    everything: bool,
) -> PyResult<Py<PyAny>> {
    let g = Gates::resolve(replay, skill_damage, timeseries, missiles, rotation, modifiers, everything);
    let bytes = std::fs::read(path).map_err(io_err)?;
    let (report_v1, ei_replay) = build_report_and_ei_inputs_from_bytes(
        &bytes, g.replay, g.skill_damage, g.timeseries, g.missiles, g.rotation, g.modifiers,
    )?;
    let ei = axilog_ei::to_ei_json(
        &report_v1, ei_replay.as_ref(),
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
