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

/// Shared decode -> resolve -> analyze -> build_report pipeline (identical
/// to `axilog-cli`'s `Cmd::Parse` handler) over an already-read byte
/// buffer.
fn build_report_from_bytes(bytes: &[u8]) -> Result<axilog_schema::Report> {
    let raw = axilog_core::evtc::decode_raw(bytes).map_err(napi_err)?;
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    Ok(axilog_schema::build_report(&enc, &metrics, env!("CARGO_PKG_VERSION")))
}

fn report_to_value(report: &axilog_schema::Report) -> Result<Value> {
    serde_json::to_value(report).map_err(napi_err)
}

/// Parses a `.evtc`/`.zevtc` file at `path` and returns the native
/// `Report` as a plain JS object (see module docs for the field-name
/// behavior).
#[napi]
pub fn parse_file(path: String) -> Result<Value> {
    let bytes = std::fs::read(&path).map_err(napi_err)?;
    parse_buffer(bytes.into())
}

/// Parses an already-read `.evtc`/`.zevtc` buffer and returns the native
/// `Report` as a plain JS object.
#[napi]
pub fn parse_buffer(buf: Buffer) -> Result<Value> {
    let report = build_report_from_bytes(buf.as_ref())?;
    report_to_value(&report)
}

/// Parses a `.evtc`/`.zevtc` file at `path` and returns the Elite
/// Insights-compatibility JSON (`axilog_ei::to_ei_json`) as a plain JS
/// object.
#[napi]
pub fn parse_file_ei(path: String) -> Result<Value> {
    let bytes = std::fs::read(&path).map_err(napi_err)?;
    let report = build_report_from_bytes(&bytes)?;
    Ok(axilog_ei::to_ei_json(&report))
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
