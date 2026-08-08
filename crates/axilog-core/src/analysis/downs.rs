use crate::analysis::PlayerMetrics;
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::BTreeSet;

/// Stub: filled in by Task 10 (downs / kills / down-contribution).
pub fn apply(
    players: &mut [PlayerMetrics],
    enc: &Encounter,
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) {
    let _ = (players, enc, raw, squad, enemies);
}
