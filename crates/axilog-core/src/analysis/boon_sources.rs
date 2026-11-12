//! Boon generation split by the skill that (probably) caused it -- an
//! ESTIMATE, never a measurement.
//!
//! arcdps' post-rework `BUFF_APPLY` statechange carries src, dst, duration
//! and buff id, but no causing skill, so there is nothing to read. What IS
//! recorded is the applier's own casts ([`super::rotation`], inferred instant
//! casts included), and most boon-granting skills apply their boon at a fixed
//! delay after the cast starts: Symbol of Luminance gives Resistance ~350ms
//! after the cast, Chapter 4: Stalwart Stand pulses at +200/+1200/+2200ms,
//! Shield of Courage gives Stability at 0ms.
//!
//! Those delays are learned offline across thousands of real WvW logs (see
//! `examples/boon_source_signatures.rs`, which writes
//! [`signatures::SIGNATURES`]) rather than per log, because one fight rarely
//! has enough casts of one skill to separate signal from coincidence.
//!
//! Per applier and boon, every application within [`BURST_MS`] of the
//! previous one is one "burst" (one skill hitting five allies). A burst is
//! credited to a skill when exactly one of the applier's casts sits at one of
//! that skill's learned lags (+/- one bin). A burst with no match that lands
//! ~1s or ~2s after a credited burst of the same boon inherits that skill
//! (pulsing fields whose later pulses drift outside the learned bins).
//! Everything else -- trait and relic procs, which arcdps gives no trace of,
//! unmatched pulses, and the rare burst two skills both match -- goes to
//! [`BoonSourceSplit::other_ms`]. On real logs that bucket is 45-70% of a
//! boon's generation, so consumers MUST show it.
//!
//! Post-rework logs only: earlier builds encode buff applications
//! differently and the signatures were learned on post-rework data.

use super::buffs::BOON_IDS;
use super::skill_map::WEAPON_SWAP_SKILL_ID;
use super::PlayerMetrics;
use crate::evtc::{event::sc, RawLog};
use crate::model::Encounter;
use std::collections::{BTreeMap, HashMap};

pub mod signatures;

/// Lag histogram bin width. A signature lists bin indices (`lag / BIN_MS`,
/// floored), and a cast matches a burst when the burst falls in a listed bin
/// or either neighbour.
pub const BIN_MS: i64 = 50;
/// Applications from one source of one boon this close together are one burst.
pub const BURST_MS: i64 = 40;
/// Casts this far before a burst are considered at all.
pub const MAX_LAG_MS: i64 = 4000;
/// Inferred instant casts can be stamped a tick AFTER their own effect.
pub const MIN_LAG_MS: i64 = -100;
/// Gap windows (ms after the last credited burst of the same boon) that
/// continue a pulsing field.
const PULSE_WINDOWS: [(i64, i64); 2] = [(800, 1250), (1800, 2250)];

/// One applier's generation of one boon, split by skill.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoonSourceSplit {
    /// skill id -> applied duration (ms, summed over every target; for the
    /// intensity boons that is stack-ms).
    pub by_skill: BTreeMap<u32, u64>,
    /// Applied duration no single skill could be credited with.
    pub other_ms: u64,
}

impl BoonSourceSplit {
    pub fn total_ms(&self) -> u64 {
        self.other_ms + self.by_skill.values().sum::<u64>()
    }
}

/// One applier's casts and boon bursts on the `t0`-relative clock
/// [`super::rotation::Cast::cast_time_ms`] uses.
#[derive(Debug, Clone, Default)]
pub struct SourceStream {
    /// Representative agent addr of the applier.
    pub agent_addr: u64,
    pub elite_spec: String,
    /// `(cast start ms, skill id)`, sorted.
    pub casts: Vec<(i64, u32)>,
    pub bursts: Vec<Burst>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Burst {
    pub time_ms: i64,
    pub boon_id: u32,
    pub targets: u32,
    /// Summed applied duration across the burst's targets.
    pub duration_ms: u64,
}

/// Squad players' casts and boon bursts. Empty for pre-rework logs.
pub fn collect_streams(
    enc: &Encounter,
    raw: &RawLog,
    players: &[PlayerMetrics],
) -> Vec<SourceStream> {
    if !raw.header.is_post_buff_rework() {
        return Vec::new();
    }
    let t0 = raw.events.first().map(|e| e.time as i64).unwrap_or(0);
    let mut idx_of: HashMap<u64, usize> = HashMap::new();
    let mut streams: Vec<SourceStream> = Vec::new();
    for p in enc.players.iter().filter(|p| p.in_squad) {
        let i = streams.len();
        streams.push(SourceStream {
            agent_addr: p.agent_addr,
            elite_spec: p.elite_spec.clone(),
            ..Default::default()
        });
        idx_of.insert(p.agent_addr, i);
        for &a in &p.agent_addrs {
            idx_of.insert(a, i);
        }
    }
    for pm in players {
        let Some(&i) = idx_of.get(&pm.agent_addr) else {
            continue;
        };
        for sr in &pm.rotation {
            if sr.skill_id == WEAPON_SWAP_SKILL_ID {
                continue;
            }
            streams[i]
                .casts
                .extend(sr.casts.iter().map(|c| (c.cast_time_ms, sr.skill_id)));
        }
    }
    let mut open: HashMap<(usize, u32), usize> = HashMap::new();
    for e in &raw.events {
        if e.is_statechange != sc::BUFF_APPLY || !BOON_IDS.iter().any(|b| b.0 == e.skillid) {
            continue;
        }
        let Some(&i) = idx_of.get(&e.src_agent) else {
            continue;
        };
        let t = e.time as i64 - t0;
        let dur = e.value.max(0) as u64;
        let s = &mut streams[i];
        if let Some(&bi) = open.get(&(i, e.skillid)) {
            let b = &mut s.bursts[bi];
            if t - b.time_ms <= BURST_MS {
                b.targets += 1;
                b.duration_ms += dur;
                continue;
            }
        }
        s.bursts.push(Burst {
            time_ms: t,
            boon_id: e.skillid,
            targets: 1,
            duration_ms: dur,
        });
        open.insert((i, e.skillid), s.bursts.len() - 1);
    }
    for s in &mut streams {
        s.casts.sort_unstable();
    }
    streams
}

/// A learned signature lookup: `(skill id, boon id) -> lag bins`.
pub type SignatureTable = HashMap<(u32, u32), &'static [i16]>;

pub fn builtin_table() -> SignatureTable {
    signatures::SIGNATURES
        .iter()
        .map(|&(s, b, bins)| ((s, b), bins))
        .collect()
}

/// The skill one burst is credited to, or `None` for the `other` bucket.
/// `last_credit` is the stream's most recent credited burst of the same boon.
fn credit(
    stream: &SourceStream,
    burst: &Burst,
    table: &SignatureTable,
    last_credit: Option<(i64, u32)>,
) -> Option<u32> {
    let lo = stream
        .casts
        .partition_point(|c| c.0 < burst.time_ms - MAX_LAG_MS);
    let mut found: Option<u32> = None;
    for &(ct, skill) in &stream.casts[lo..] {
        if ct > burst.time_ms - MIN_LAG_MS {
            break;
        }
        let Some(bins) = table.get(&(skill, burst.boon_id)) else {
            continue;
        };
        let bin = (burst.time_ms - ct).div_euclid(BIN_MS);
        if bins.iter().any(|&b| (b as i64 - bin).abs() <= 1) {
            match found {
                None => found = Some(skill),
                Some(f) if f == skill => {}
                // Two different skills both fit: refuse to guess.
                Some(_) => return None,
            }
        }
    }
    found.or_else(|| {
        let (t, skill) = last_credit?;
        let gap = burst.time_ms - t;
        PULSE_WINDOWS
            .iter()
            .any(|&(a, b)| (a..=b).contains(&gap))
            .then_some(skill)
    })
}

/// Split every stream's boon generation by skill.
/// Keyed by `(applier representative addr, boon id)`.
pub fn attribute(
    streams: &[SourceStream],
    table: &SignatureTable,
) -> BTreeMap<(u64, u32), BoonSourceSplit> {
    let mut out: BTreeMap<(u64, u32), BoonSourceSplit> = BTreeMap::new();
    for s in streams {
        let mut last: HashMap<u32, (i64, u32)> = HashMap::new();
        for burst in &s.bursts {
            let split = out.entry((s.agent_addr, burst.boon_id)).or_default();
            match credit(s, burst, table, last.get(&burst.boon_id).copied()) {
                Some(skill) => {
                    *split.by_skill.entry(skill).or_default() += burst.duration_ms;
                    last.insert(burst.boon_id, (burst.time_ms, skill));
                }
                None => split.other_ms += burst.duration_ms,
            }
        }
    }
    out
}

/// [`collect_streams`] + [`attribute`] against the built-in signatures.
pub fn compute(
    enc: &Encounter,
    raw: &RawLog,
    players: &[PlayerMetrics],
) -> BTreeMap<(u64, u32), BoonSourceSplit> {
    attribute(&collect_streams(enc, raw, players), &builtin_table())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESIST: u32 = 26980;
    const LUMINANCE: u32 = 73132;
    const STALWART: u32 = 40988;

    fn table() -> SignatureTable {
        static LUM: [i16; 1] = [7];
        static STAL: [i16; 3] = [4, 24, 44];
        [
            ((LUMINANCE, RESIST), &LUM[..]),
            ((STALWART, RESIST), &STAL[..]),
        ]
        .into_iter()
        .collect()
    }

    fn burst(t: i64, dur: u64) -> Burst {
        Burst {
            time_ms: t,
            boon_id: RESIST,
            targets: 5,
            duration_ms: dur,
        }
    }

    #[test]
    fn credits_matching_lag_and_buckets_the_rest() {
        let s = SourceStream {
            agent_addr: 1,
            casts: vec![(1_000, LUMINANCE), (10_000, STALWART)],
            bursts: vec![
                burst(1_350, 100), // Luminance +350
                burst(10_200, 50), // Stalwart +200
                burst(20_000, 7),  // no cast near: trait proc
            ],
            ..Default::default()
        };
        let got = attribute(&[s], &table());
        let split = &got[&(1, RESIST)];
        assert_eq!(split.by_skill[&LUMINANCE], 100);
        assert_eq!(split.by_skill[&STALWART], 50);
        assert_eq!(split.other_ms, 7);
        assert_eq!(split.total_ms(), 157);
    }

    #[test]
    fn neighbour_bin_matches_but_two_skills_do_not() {
        let s = SourceStream {
            agent_addr: 1,
            // Luminance +400 (bin 8, neighbour of 7) and Stalwart +200 both fit.
            casts: vec![(600, LUMINANCE), (800, STALWART)],
            bursts: vec![burst(1_000, 10)],
            ..Default::default()
        };
        let got = attribute(&[s], &table());
        assert_eq!(got[&(1, RESIST)].other_ms, 10);
        assert!(got[&(1, RESIST)].by_skill.is_empty());
    }

    #[test]
    fn pulse_chaining_inherits_previous_credit() {
        let s = SourceStream {
            agent_addr: 1,
            casts: vec![(0, LUMINANCE)],
            bursts: vec![
                burst(350, 1),
                burst(1_400, 2),
                burst(2_400, 3),
                burst(5_000, 4),
            ],
            ..Default::default()
        };
        let got = attribute(&[s], &table());
        let split = &got[&(1, RESIST)];
        assert_eq!(split.by_skill[&LUMINANCE], 6);
        assert_eq!(split.other_ms, 4);
    }
}
