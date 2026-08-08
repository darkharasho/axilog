use crate::analysis::{PlayerMetrics, Timeline};
use crate::evtc::RawLog;
use crate::model::Encounter;
use std::collections::BTreeSet;

/// Stub: filled in by Task 11 (CC timeline).
pub fn timeline(
    enc: &Encounter,
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) -> Timeline {
    let _ = (enc, raw, squad, enemies);
    Timeline { resolution_ms: 1000, squad_damage: Vec::new(), cc_applied: Vec::new(), downs: Vec::new() }
}

/// Stub: filled in by Task 11 (per-player CC application).
pub fn apply_cc(
    players: &mut [PlayerMetrics],
    raw: &RawLog,
    squad: &BTreeSet<u64>,
    enemies: &BTreeSet<u64>,
) {
    let _ = (players, raw, squad, enemies);
}
