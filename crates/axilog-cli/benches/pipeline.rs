//! MPERF Milestone, Task 1: criterion measurement baseline.
//!
//! Pure measurement -- this file does not change any analysis/parsing code.
//! It benchmarks each stage of the axilog pipeline separately (decode ->
//! resolve -> analyze), plus the full pipeline through
//! `axilog_schema::build_report`, over the committed anonymized fixture
//! `fixtures/wvw-small.anon.zevtc` (always present, so this arm always
//! runs -- in CI and locally).
//!
//! A second, env-gated arm (`AXILOG_BENCH_LOG=<path>`) benchmarks the same
//! stages over a real, larger log a developer has locally. That file is
//! never committed (see `fixtures/local/` in `.gitignore` -- real logs
//! contain player account names, which is PII); when the env var is unset,
//! or points at a file that doesn't exist, this arm prints a note to
//! stderr and returns without registering any benchmark -- it never panics
//! and never fails the run, so `cargo bench` (and CI's `cargo bench --no-run`
//! / smoke run) stay green with no local log present.
//!
//! Bench location: this lives in `axilog-cli` rather than `axilog-core`
//! because the full pipeline needs `axilog_schema::build_report`, and
//! `axilog-cli` already depends on both `axilog-core` and `axilog-schema`
//! as regular dependencies -- benchmarking here adds exactly one new
//! dependency edge (criterion, dev-only) instead of introducing a new
//! axilog-core -> axilog-schema dev-dependency edge that doesn't exist
//! today.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

/// Committed, anonymized, always-available fixture (Task 5 / M2's
/// `anonymize` subcommand product). Resolved relative to this crate's
/// manifest dir so `cargo bench` works regardless of the invoking cwd.
const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/wvw-small.anon.zevtc"
);

/// Registers `decode_raw` / `model::resolve` / `analysis::analyze` /
/// `full_pipeline` benchmarks under `group_name`, over `bytes`. Each stage
/// benchmark measures only its own stage's cost -- upstream stages are
/// computed once outside the `b.iter` closure and fed in by reference,
/// except `full_pipeline`, which intentionally redoes every stage inside
/// the closure to measure the whole thing end to end (matching what the
/// CLI's `axilog parse` actually does per invocation).
fn bench_pipeline(c: &mut Criterion, group_name: &str, bytes: &[u8], sample_size: Option<usize>) {
    let mut group = c.benchmark_group(group_name);
    if let Some(n) = sample_size {
        group.sample_size(n);
    }

    group.bench_function("decode_raw", |b| {
        b.iter(|| {
            let raw = axilog_core::evtc::decode_raw(black_box(bytes)).expect("decode_raw");
            black_box(raw)
        });
    });

    let raw = axilog_core::evtc::decode_raw(bytes).expect("decode_raw");

    group.bench_function("model::resolve", |b| {
        b.iter(|| {
            let enc = axilog_core::model::resolve(black_box(&raw));
            black_box(enc)
        });
    });

    let enc = axilog_core::model::resolve(&raw);

    group.bench_function("analysis::analyze", |b| {
        b.iter(|| {
            let metrics = axilog_core::analysis::analyze(black_box(&enc), black_box(&raw));
            black_box(metrics)
        });
    });

    group.bench_function("full_pipeline (decode+resolve+analyze+build_report)", |b| {
        b.iter(|| {
            let raw = axilog_core::evtc::decode_raw(black_box(bytes)).expect("decode_raw");
            let enc = axilog_core::model::resolve(&raw);
            let metrics = axilog_core::analysis::analyze(&enc, &raw);
            // Full pipeline, always-on path: no replay/missiles blocks, and
            // every opt-in Report field (skill_damage/timeseries/rotation)
            // off, matching a plain `axilog parse` with no flags -- see the
            // task brief's instruction to pass None/false for opt-in
            // blocks so this measures the always-on path.
            let report = axilog_schema::build_report(
                &enc, &metrics, "bench", None, None, false, false, false, None,
            );
            black_box(report)
        });
    });

    group.finish();
}

fn bench_fixture(c: &mut Criterion) {
    let bytes = std::fs::read(FIXTURE_PATH)
        .unwrap_or_else(|e| panic!("read committed fixture {FIXTURE_PATH}: {e}"));
    bench_pipeline(c, "fixture/wvw-small", &bytes, None);
}

/// Env-gated real-log arm. See module doc comment for the PII/gitignore
/// rationale and the "skip cleanly" contract.
fn bench_real_log(c: &mut Criterion) {
    let path = match std::env::var("AXILOG_BENCH_LOG") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("AXILOG_BENCH_LOG not set -- skipping real-log benchmark arm");
            return;
        }
    };
    if !std::path::Path::new(&path).exists() {
        eprintln!("AXILOG_BENCH_LOG={path} does not exist -- skipping real-log benchmark arm");
        return;
    }
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("AXILOG_BENCH_LOG={path} could not be read ({e}) -- skipping real-log benchmark arm");
            return;
        }
    };
    // Real logs are much larger (the milestone brief references a 583k-event
    // local log) -- a lower sample size keeps a full `cargo bench` run
    // finishing in a reasonable time locally.
    bench_pipeline(c, "real_log", &bytes, Some(10));
}

criterion_group!(benches, bench_fixture, bench_real_log);
criterion_main!(benches);
