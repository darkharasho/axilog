//! Learns the `(skill, boon) -> lag bins` table behind
//! `analysis::boon_sources` from real logs and writes it as Rust source.
//!
//! Usage:
//!   boon_source_signatures <train-list> [<holdout-list>] > \
//!     crates/axilog-core/src/analysis/boon_sources/signatures.rs
//!
//! `BS_DEBUG=<skill>,<boon>` prints that pair's top lag bins to stderr.
//!
//! Each list file holds one .zevtc/.evtc path per line; only post-rework WvW
//! logs are used. With a holdout list, per-boon coverage (share of generated
//! duration credited to a skill) is printed to stderr.
//!
//! A bin becomes part of a skill's signature when, over every cast of that
//! skill by any squad player, a burst of the boon lands in that bin on at
//! least [`MIN_RATE`] of casts (a bin's
//! count includes its two neighbours, since matching tolerates +/- one bin) AND at least [`MIN_LIFT`] times the skill's
//! average bin rate for that boon (so a skill that is merely cast during a
//! pulsing field doesn't learn the field's cadence).
use axilog_core::analysis::{
    self,
    boon_sources::{self as bs, SourceStream},
    buffs::BOON_IDS,
};
use axilog_core::evtc::decode_raw;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

const MIN_CASTS: u32 = 30;
const MIN_RATE: f64 = 0.30;
const MIN_LIFT: f64 = 4.0;

fn load(list: &str, names: &mut BTreeMap<u32, String>, mut each: impl FnMut(Vec<SourceStream>)) {
    let mut players = 0;
    let mut logs = 0;
    for path in std::fs::read_to_string(list)
        .expect("list file")
        .lines()
        .filter(|l| !l.is_empty())
    {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(raw) = decode_raw(&bytes) else {
            continue;
        };
        if !raw.header.is_post_buff_rework() {
            continue;
        }
        let enc = axilog_core::model::resolve(&raw);
        if enc.kind != "wvw" {
            continue;
        }
        for s in &raw.skills {
            names.entry(s.id).or_insert_with(|| s.name.clone());
        }
        let m = analysis::analyze(&enc, &raw);
        let streams = bs::collect_streams(&enc, &raw, &m.players);
        players += streams.len();
        each(streams);
        logs += 1;
    }
    eprintln!("{list}: {logs} logs, {players} squad players");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let train = args
        .next()
        .expect("usage: boon_source_signatures <train-list> [<holdout-list>]");
    let holdout = args.next();
    let mut names = BTreeMap::new();
    let lo_bin = bs::MIN_LAG_MS.div_euclid(bs::BIN_MS);
    let hi_bin = bs::MAX_LAG_MS.div_euclid(bs::BIN_MS);
    let mut casts: HashMap<u32, u32> = HashMap::new();
    let mut hist: HashMap<(u32, u32), HashMap<i64, u32>> = HashMap::new();
    let mut n_streams = 0usize;
    load(&train, &mut names, |streams| {
        n_streams += streams.len();
        for s in &streams {
            for &(ct, skill) in &s.casts {
                *casts.entry(skill).or_default() += 1;
                let first = s
                    .bursts
                    .partition_point(|b| b.time_ms < ct + bs::MIN_LAG_MS);
                let mut seen = BTreeSet::new();
                for b in &s.bursts[first..] {
                    let lag = b.time_ms - ct;
                    if lag > bs::MAX_LAG_MS {
                        break;
                    }
                    let bin = lag.div_euclid(bs::BIN_MS);
                    if seen.insert((b.boon_id, bin)) {
                        *hist
                            .entry((skill, b.boon_id))
                            .or_default()
                            .entry(bin)
                            .or_default() += 1;
                    }
                }
            }
        }
    });
    if let Ok(v) = std::env::var("BS_DEBUG") {
        let mut it = v.split(',').map(|x| x.parse::<u32>().unwrap());
        let (sk, bo) = (it.next().unwrap(), it.next().unwrap());
        let n = casts.get(&sk).copied().unwrap_or(0);
        let mut rows: Vec<_> = hist.get(&(sk, bo)).map(|h| h.iter().map(|(&b, &c)| (c, b)).collect()).unwrap_or_default();
        rows.sort_unstable_by(|a: &(u32, i64), b| b.cmp(a));
        let tot: u32 = rows.iter().map(|r| r.0).sum();
        eprintln!("debug {sk}/{bo}: casts={n} mean_bin_rate={:.3}", tot as f64 / ((hi_bin - lo_bin + 1) as f64) / n.max(1) as f64);
        for (c, b) in rows.iter().take(8) { eprintln!("  bin {b} ({}ms): {:.1}%", b * bs::BIN_MS, 100.0 * *c as f64 / n.max(1) as f64); }
    }
    let n_bins = (hi_bin - lo_bin + 1) as f64;
    let mut sigs: BTreeMap<(u32, u32), (Vec<i16>, f64, u32)> = BTreeMap::new();
    for (&(skill, boon), h) in &hist {
        let n = casts[&skill];
        if n < MIN_CASTS {
            continue;
        }
        let mean = h.values().map(|&c| c as f64).sum::<f64>() / n_bins;
        // Matching tolerates +/- one bin, so score each bin with its
        // neighbours and keep local peaks only.
        let at = |b: i64| h.get(&b).copied().unwrap_or(0) as f64;
        let score = |b: i64| at(b - 1) + at(b) + at(b + 1);
        let mut bins: Vec<i16> = (lo_bin..=hi_bin)
            .filter(|&b| {
                let sc = score(b);
                sc >= MIN_RATE * n as f64
                    && sc >= MIN_LIFT * 3.0 * mean
                    && sc >= score(b - 1)
                    && sc > score(b + 1)
            })
            .map(|b| b as i16)
            .collect();
        if bins.is_empty() {
            continue;
        }
        bins.sort_unstable();
        let best = bins.iter().map(|&b| score(b as i64) as u32).max().unwrap_or(0);
        sigs.insert((skill, boon), (bins, best as f64 / n as f64, n));
    }

    let boon_name: HashMap<u32, &str> = BOON_IDS.iter().map(|&(id, n, _)| (id, n)).collect();
    let mut src = String::new();
    src.push_str(
        "// @generated by `cargo run --release -p axilog-core --example boon_source_signatures`.\n",
    );
    src.push_str("// Do not edit by hand; regenerate from real post-rework WvW logs.\n");
    let _ = writeln!(
        src,
        "// Learned from {} squad-player streams; bins are {}ms wide, lag = burst - cast start.",
        n_streams,
        bs::BIN_MS
    );
    src.push_str(
        "//\n// (skill id, boon id, lag bins)  // skill, boon, best hit rate (bin +/- 1; can exceed 100%), casts\n",
    );
    src.push_str("#[rustfmt::skip]\npub static SIGNATURES: &[(u32, u32, &[i16])] = &[\n");
    for (&(skill, boon), (bins, rate, n)) in &sigs {
        let name = names.get(&skill).map(String::as_str).unwrap_or("");
        let _ = writeln!(
            src,
            "    ({skill}, {boon}, &{bins:?}), // {name:?} {} {:.0}% n={n}",
            boon_name.get(&boon).copied().unwrap_or("?"),
            rate * 100.0
        );
    }
    src.push_str("];\n");
    print!("{src}");
    eprintln!("{} signatures", sigs.len());

    if let Some(h) = holdout {
        let mut streams = Vec::new();
        load(&h, &mut names, |s| streams.extend(s));
        let table: bs::SignatureTable = sigs
            .iter()
            .map(|(&k, (bins, ..))| (k, &*Box::leak(bins.clone().into_boxed_slice())))
            .collect();
        let split = bs::attribute(&streams, &table);
        let mut per: BTreeMap<u32, (u64, u64)> = BTreeMap::new();
        for ((_, boon), s) in &split {
            let e = per.entry(*boon).or_default();
            e.0 += s.total_ms() - s.other_ms;
            e.1 += s.total_ms();
        }
        eprintln!("holdout coverage (share of generated duration credited to a skill):");
        for (boon, (c, t)) in per {
            eprintln!(
                "  {:<13} {:>5.1}%",
                boon_name.get(&boon).copied().unwrap_or("?"),
                100.0 * c as f64 / t.max(1) as f64
            );
        }
    }
}
