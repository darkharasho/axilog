//! napi-rs Node addon bindings for axilog's Rust parsing core (M5 Task 1).
//!
//! Every export here is a thin wrapper around the same pipeline the CLI
//! (`crates/axilog-cli/src/main.rs`) drives: `evtc::decode_raw` ->
//! `model::resolve` -> `analysis::analyze` -> `axilog_schema::build_report`,
//! optionally through `axilog_ei::to_ei_json`. No Rust panic is allowed to
//! cross the FFI boundary: every fallible step (`io::Error`,
//! `axilog_core::evtc::EvtcError`) is mapped to a `napi::Error` carrying the
//! original Rust error text via `Display`/`ToString`, never `.unwrap()`ed.
//!
//! Return values are `serde_json::Value` (the `napi`/`serde-json` cargo
//! feature), not a `#[napi(object)]`-derived struct. napi's serde-json
//! interop converts a `Value::Object` into a plain JS object key-by-key
//! (see `napi::bindgen_runtime::js_values::serde`'s `ToNapiValue for
//! Value` impl), so the JS object's keys are exactly the serde field names
//! `Report`/`to_ei_json` already produced (snake_case, e.g.
//! `schema_version`) -- napi never re-cases them, unlike the camelCase
//! rewriting `#[napi(object)]` applies to derived-struct field names.
#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json::Value;

/// Rust-side error text, unwrapped from whatever `Display` the underlying
/// error type provides -- `napi::Error::from_reason` takes a plain string,
/// so every fallible step below is mapped through this instead of ever
/// unwinding across the FFI boundary.
fn napi_err(reason: impl std::fmt::Display) -> Error {
    Error::from_reason(reason.to_string())
}

/// Optional per-call parse settings (M9, Task 2). `replay: true` opts into
/// computing and embedding the native combat-replay block (`ReplayOut`) in
/// the returned `Report`; omitted (or `false`, or the argument itself
/// omitted entirely -- napi treats a trailing `Option<T>` parameter as
/// optional in the generated TypeScript signature) keeps the existing
/// zero-arg call shape's behavior unchanged (no `replay` key in the
/// output, matching `Report.replay`'s serde skip-when-absent).
/// `skill_damage: true` (M12, Task 1) opts into embedding the native
/// per-skill damage distribution block (`SkillDamageOut`) on every
/// `players[]` entry -- see `axilog_schema::Report::players`'s
/// `PlayerOut::skill_damage` doc comment for why this defaults to opt-in
/// (measured +249% JSON size on the committed fixture when always-on).
/// `timeseries: true` (M12, Task 2) opts into embedding the native
/// per-player per-second series block (`PlayerPerSecondOut`) AND the
/// per-enemy `dps_targets` summary on every `players[]` entry -- see
/// `axilog_schema::Report::players`'s `PlayerOut::per_second`/`PlayerOut::
/// dps_targets` doc comments (measured +147.7%/+36.4% JSON size
/// respectively on the committed fixture when always-on -- `dps_targets`
/// is NOT small on a real WvW log with many enemies, so both stay behind
/// this one flag).
/// `missiles: true` (final-review fix wave) opts into embedding the
/// native top-level missile (projectile) analytics block
/// (`Report::missiles`), mirroring the CLI's `--missiles` flag -- see
/// `axilog_core::analysis::missiles`'s module doc for exactly what it
/// contains. Omitted (or `false`) keeps `Report.missiles` absent, matching
/// its serde skip-when-`None`.
/// `rotation: true` (M14, Task 1) opts into embedding the native
/// per-player rotation (cast tracking) block (`SkillRotationOut[]`) on
/// every `players[]` entry -- see `axilog_schema::Report::players`'s
/// `PlayerOut::rotation` doc comment for why this defaults to opt-in
/// (well past the ~30% size-discipline guideline on the committed fixture
/// when always-on).
/// `modifiers: true` (M16) opts into the per-player damage-modifier stats
/// -- `players[].damage_mods.{outgoing, incoming}` plus the top-level
/// `damage_mod_map` on the native `Report` (`parseFile`/`parseBuffer`), and
/// EI's own `damageModifiers`/`incomingDamageModifiers`/
/// `damageModifiersTarget`/`incomingDamageModifiersTarget` plus
/// `damageModMap` on `parseFileEi`. Unlike every other option here this one
/// gates a COMPUTATION rather than a copy: `analyze()` never runs the
/// modifier engine, which is a separate pass over every damage event
/// crossed with ~200 catalogued definitions (see
/// `axilog_schema::DamageModsOut`'s doc comment). Off by default.
#[napi(object)]
#[derive(Default, Clone, Copy)]
pub struct ParseOptions {
    pub replay: Option<bool>,
    pub skill_damage: Option<bool>,
    pub timeseries: Option<bool>,
    pub missiles: Option<bool>,
    pub rotation: Option<bool>,
    pub modifiers: Option<bool>,
}

/// Shared decode -> resolve -> analyze -> build_report -> build_report_v1
/// pipeline (identical up through `build_report` to `axilog-cli`'s
/// `Cmd::Parse` handler) over an already-read byte buffer, returning the
/// native 1.0 container (Task 12: the native entry points -- `parseFile`/
/// `parseBuffer` -- emit the 1.0 document, mirroring the CLI's `--format
/// json`; `parseFileEi` is untouched and keeps consuming the legacy
/// `Report` via `build_report_and_activity_from_bytes` below).
/// `want_replay` mirrors `ParseOptions.replay`; `want_skill_damage` mirrors
/// `ParseOptions.skill_damage`; `want_missiles` mirrors
/// `ParseOptions.missiles` (all defaulted to `false` by callers that pass
/// no options at all). `generated_from` is the origin file NAME (never a
/// full path -- paths are environment-specific and routinely contain a
/// user name, which the PII policy scrubs); `parseBuffer` has no file name
/// to offer and passes `None`.
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
) -> Result<axilog_schema::v1::ReportV1> {
    let raw = axilog_core::evtc::decode_raw(bytes).map_err(napi_err)?;
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
        damage_mods.as_ref(),
    ))
}

/// Same decode -> resolve -> analyze pipeline as `build_report_from_bytes`,
/// but additionally returns the M11 Task 3 activity intervals
/// (`axilog_core::analysis::replay::build_activity_intervals`) the ei-json
/// adapter needs for `combatReplayData`/`activeTimes` -- computed
/// unconditionally (cheap, unlike `--replay`'s position track), independent
/// of `want_replay`.
///
/// `want_skill_damage`/`want_timeseries` (final-review fix wave) are
/// threaded into the `build_report` call below so `parseFileEi`'s
/// `{ skillDamage: true }`/`{ timeseries: true }` options can actually
/// surface `totalDamageDist`/`damage1S`/etc in the returned ei-json (see
/// `axilog_ei::to_ei_json`, which reads those fields straight off
/// `PlayerOut::skill_damage`/`PlayerOut::per_second` -- previously always
/// `None` here regardless of what the caller asked for). `want_missiles`
/// is threaded the same way for symmetry with `parse_buffer`/`parse_file`,
/// even though `to_ei_json` does not currently read `Report::missiles`.
#[allow(clippy::type_complexity)]
fn build_report_and_activity_from_bytes(
    bytes: &[u8],
    want_replay: bool,
    want_skill_damage: bool,
    want_timeseries: bool,
    want_missiles: bool,
    want_rotation: bool,
    want_modifiers: bool,
) -> Result<(
    axilog_schema::Report,
    axilog_schema::v1::ReportV1,
    Vec<axilog_core::analysis::replay::ActivityIntervals>,
    Option<axilog_core::analysis::ei_replay::EiReplay>,
    Option<axilog_core::analysis::damage_mods::DamageModifierResults>,
    Option<axilog_core::analysis::buffs::BoonStates>,
    Option<std::collections::BTreeMap<u64, axilog_core::analysis::timeseries::EnemySeries>>,
    Option<std::collections::BTreeMap<u64, Vec<axilog_core::analysis::skill_damage::SkillEntry>>>,
    Option<axilog_core::analysis::target_conditions::TargetConditionStates>,
    Option<axilog_core::analysis::healing_detail::HealingDetail>,
    Option<axilog_core::analysis::minions::MinionRollups>,
    Option<std::collections::BTreeMap<u64, axilog_core::analysis::dist_outcomes::DistOutcomes>>,
    Option<std::collections::BTreeMap<u64, Vec<(u64, f64)>>>,
)> {
    let raw = axilog_core::evtc::decode_raw(bytes).map_err(napi_err)?;
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
    // M15 Task 3: `opts.replay` now DOES affect the ei-json -- it adds
    // `combatReplayData.{positions, orientations, dc, iconURL}` and the
    // top-level `combatReplayMetaData`. Same flag, same opt-in cost
    // rationale as the native block above (see `axilog_ei::to_ei_json`'s
    // measured size delta).
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
    // `report`. `parseFileEi`/`parseBufferEi` have no file name to offer
    // (unlike `parse_file`'s own `build_report_v1_from_bytes`), so
    // `generated_from` stays `None`, matching this function's existing
    // convention.
    let report_v1 = axilog_schema::v1::build_report_v1(
        &enc, &metrics, &report, env!("CARGO_PKG_VERSION"), None, damage_mods.as_ref(),
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
    // `JsonActorBuilder.cs:63` and on `statesPerSource` at
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
    // MEIGAP Task 3a/3b. Every healing-detail family is flag-gated in the
    // adapter (`healing1S` on timeseries; the ally matrices and the two
    // `*Dist` arrays on skill-damage -- see `EiInputs::healing_dist`), so
    // the pass only runs when at least one of them will be serialized. It
    // self-gates to `None` on a log with no healing extension.
    let healing_detail = (want_skill_damage || want_timeseries)
        .then(|| axilog_core::analysis::healing_detail::build(&raw, &enc))
        .flatten();
    let minion_rollups =
        want_skill_damage.then(|| axilog_core::analysis::minions::build(&raw, &enc));
    // MEIGAP2 rows 1 and 2 -- same gates the CLI uses (`--skill-damage`
    // for the distributions' outcome columns, `--timeseries` for the
    // health series, GW2EI's own `RawFormatTimelineArrays`).
    let dist_outcomes =
        want_skill_damage.then(|| axilog_core::analysis::dist_outcomes::build(&raw, &enc));
    let health_percents =
        want_timeseries.then(|| axilog_core::analysis::health::ei_health_percents(&raw, &enc));
    Ok((
        report,
        report_v1,
        activity,
        ei_replay,
        damage_mods,
        boon_states,
        enemy_series,
        enemy_dist,
        target_conditions,
        healing_detail,
        minion_rollups,
        dist_outcomes,
        health_percents,
    ))
}

fn report_v1_to_value(report: &axilog_schema::v1::ReportV1) -> Result<Value> {
    serde_json::to_value(report).map_err(napi_err)
}

/// Parses a `.evtc`/`.zevtc` file at `path` and returns the native 1.0
/// container (Task 12) as a plain JS object (see module docs for the
/// field-name behavior). `opts.replay` (M9, Task 2) opts into embedding the
/// native combat-replay block; omitted entirely for back-compat with every
/// existing zero-arg call site. Unlike `parseBuffer`, this entry point has
/// a real file name to offer, so it threads `path`'s file name (never the
/// full path -- see `build_report_v1_from_bytes`'s doc comment) into the
/// document's `generated_from`.
#[napi]
pub fn parse_file(path: String, opts: Option<ParseOptions>) -> Result<Value> {
    let bytes = std::fs::read(&path).map_err(napi_err)?;
    let want_replay = opts.and_then(|o| o.replay).unwrap_or(false);
    let want_skill_damage = opts.and_then(|o| o.skill_damage).unwrap_or(false);
    let want_timeseries = opts.and_then(|o| o.timeseries).unwrap_or(false);
    let want_missiles = opts.and_then(|o| o.missiles).unwrap_or(false);
    let want_rotation = opts.and_then(|o| o.rotation).unwrap_or(false);
    let want_modifiers = opts.and_then(|o| o.modifiers).unwrap_or(false);
    let generated_from =
        std::path::Path::new(&path).file_name().and_then(|s| s.to_str());
    let report = build_report_v1_from_bytes(
        &bytes, want_replay, want_skill_damage, want_timeseries, want_missiles, want_rotation,
        want_modifiers, generated_from,
    )?;
    report_v1_to_value(&report)
}

/// Parses an already-read `.evtc`/`.zevtc` buffer and returns the native
/// 1.0 container (Task 12) as a plain JS object. `opts.replay` (M9, Task 2)
/// opts into embedding the native combat-replay block; `opts.skill_damage`
/// (M12, Task 1) opts into embedding the native per-skill damage
/// distribution block; `opts.missiles` (final-review fix wave) opts into
/// embedding the native top-level missile analytics block. A buffer has no
/// file name to offer, so `generated_from` is always absent here.
#[napi]
pub fn parse_buffer(buf: Buffer, opts: Option<ParseOptions>) -> Result<Value> {
    let want_replay = opts.and_then(|o| o.replay).unwrap_or(false);
    let want_skill_damage = opts.and_then(|o| o.skill_damage).unwrap_or(false);
    let want_timeseries = opts.and_then(|o| o.timeseries).unwrap_or(false);
    let want_missiles = opts.and_then(|o| o.missiles).unwrap_or(false);
    let want_rotation = opts.and_then(|o| o.rotation).unwrap_or(false);
    let want_modifiers = opts.and_then(|o| o.modifiers).unwrap_or(false);
    let report = build_report_v1_from_bytes(
        buf.as_ref(), want_replay, want_skill_damage, want_timeseries, want_missiles, want_rotation,
        want_modifiers, None,
    )?;
    report_v1_to_value(&report)
}

/// Parses a `.evtc`/`.zevtc` file at `path` and returns the Elite
/// Insights-compatibility JSON (`axilog_ei::to_ei_json`) as a plain JS
/// object. `opts` (final-review fix wave) accepts the same `ParseOptions`
/// shape `parseFile`/`parseBuffer` do -- `opts.skill_damage`/
/// `opts.timeseries` are what actually let `totalDamageDist`/`damage1S`/
/// `dpsTargets`/etc (M12, Task 3's ei-json mapping) surface in the
/// returned JSON, since `axilog_ei::to_ei_json` reads them straight off
/// the native `Report` this function builds internally; previously this
/// function always built that `Report` with both flags forced `false`,
/// silently discarding any M12 detail regardless of what a caller wanted
/// (axibridge consumes ei-json exclusively through this function).
/// As of MEIGAP2 those two flags gate three more GW2EI surfaces:
/// `opts.skill_damage` additionally carries the player distributions'
/// outcome columns (`connectedHits`/`glance`/`missed`/`evaded`/`blocked`/
/// `invulned`/`interrupted`/`indirectDamage`, plus per-skill
/// `downContribution` on the outgoing one), and `opts.timeseries`
/// additionally carries `healthPercents` and `boonsStates` -- GW2EI's own
/// `RawFormatTimelineArrays` gate on both. `instanceID`,
/// `dpsAll[0].breakbarDamage` and `targets[].dpsAll` need no flag, matching
/// GW2EI, which always emits them.
/// `opts.replay` (M15, Task 3) adds GW2EI's own combat-replay surface --
/// per-actor `combatReplayData.{positions, orientations, dc, iconURL}` plus
/// the top-level `combatReplayMetaData` (see `axilog_ei::to_ei_json`; it
/// roughly triples the payload, hence opt-in). `opts.missiles` is accepted
/// for parity with `parseFile` but has no effect on the output -- EI's JSON
/// shape has no comparable field for it. Omitting `opts`
/// entirely keeps every existing zero-arg call site's behavior unchanged.
#[napi]
pub fn parse_file_ei(path: String, opts: Option<ParseOptions>) -> Result<Value> {
    let want_replay = opts.and_then(|o| o.replay).unwrap_or(false);
    let want_skill_damage = opts.and_then(|o| o.skill_damage).unwrap_or(false);
    let want_timeseries = opts.and_then(|o| o.timeseries).unwrap_or(false);
    let want_missiles = opts.and_then(|o| o.missiles).unwrap_or(false);
    let want_rotation = opts.and_then(|o| o.rotation).unwrap_or(false);
    let want_modifiers = opts.and_then(|o| o.modifiers).unwrap_or(false);
    let bytes = std::fs::read(&path).map_err(napi_err)?;
    let (report, report_v1, activity, ei_replay, damage_mods, boon_states, enemy_series, enemy_dist,
         target_conditions, healing_detail, minion_rollups, dist_outcomes, health_percents) = build_report_and_activity_from_bytes(
        &bytes, want_replay, want_skill_damage, want_timeseries, want_missiles, want_rotation,
        want_modifiers,
    )?;
    Ok(axilog_ei::to_ei_json(
        &report_v1,
        &report,
        &axilog_ei::EiInputs {
            activity: &activity,
            replay: ei_replay.as_ref(),
            modifiers: damage_mods.as_ref(),
            boon_states: boon_states.as_ref(),
            enemy_series: enemy_series.as_ref(),
            enemy_dist: enemy_dist.as_ref(),
            target_conditions: target_conditions.as_ref(),
            healing_detail: healing_detail.as_ref(),
            healing_series: want_timeseries,
            healing_dist: want_skill_damage,
            minions: minion_rollups.as_ref(),
            dist_outcomes: dist_outcomes.as_ref(),
            health_percents: health_percents.as_ref(),
        },
    ))
}

/// Rewrites every player's character/account name in the `.zevtc` at
/// `in_path` to a deterministic `Anon<N>` placeholder and writes the
/// result to `out_path` (same transform as `axilog anonymize` / the core
/// `anonymize_raw_evtc` function -- reused here, not duplicated). Returns
/// the number of player agents rewritten.
#[napi]
pub fn anonymize_file(in_path: String, out_path: String) -> Result<u32> {
    let bytes = std::fs::read(&in_path).map_err(napi_err)?;
    let mut data = axilog_core::evtc::inflate_zevtc(&bytes).map_err(napi_err)?;
    let rewritten = axilog_core::evtc::anonymize_raw_evtc(&mut data).map_err(napi_err)?;

    let entry_name = std::path::Path::new(&out_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("log");
    let zipped = axilog_core::evtc::zip_deflate(&format!("{entry_name}.evtc"), &data);
    std::fs::write(&out_path, zipped).map_err(napi_err)?;

    Ok(rewritten as u32)
}
