//! The encounter's original agent order, preserved for reprojection.
//!
//! `entities[]` is sorted for human and diff legibility -- role, team,
//! subgroup, account, character, addr (see `build_entities`). That sort
//! deliberately discards the order agents appeared in the encounter.
//!
//! Some reprojections need that discarded order back. ei-json is the
//! motivating one: its `players[]` and `targets[]` are POSITIONAL arrays,
//! and `dpsTargets`/`statsTargets`/`targetDamageDist` are indexed by
//! position within them. Recomputing the order from `entities[]` is
//! impossible -- the information is gone, not merely rearranged.
//!
//! This is `#[serde(skip)]` and never reaches the wire. Consumers of the
//! 1.0 document join by `id`; ordering is not part of the contract. The
//! precedent is `PlayerOut::agent_addr`, a non-serialized join key added
//! for the same class of reason.

use serde::Serialize;
use std::collections::BTreeMap;

/// Entity ids in the encounter's original iteration order.
///
/// `players` mirrors `Encounter::players`; `targets` mirrors the sweep
/// `Report::ei_targets` uses (the `is_player` entries of
/// `Encounter::enemies`, in encounter order).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SourceOrder {
    players: Vec<u32>,
    targets: Vec<u32>,
    #[serde(skip)]
    player_pos: BTreeMap<u32, usize>,
    #[serde(skip)]
    target_pos: BTreeMap<u32, usize>,
}

impl SourceOrder {
    /// Build from the two id sequences, in encounter order.
    pub fn new(players: Vec<u32>, targets: Vec<u32>) -> Self {
        let player_pos = players.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let target_pos = targets.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        Self { players, targets, player_pos, target_pos }
    }

    /// Entity ids in `Encounter::players` order.
    pub fn players(&self) -> &[u32] {
        &self.players
    }

    /// Entity ids in `Report::ei_targets` order.
    pub fn targets(&self) -> &[u32] {
        &self.targets
    }

    /// This entity's slot in `players()`, if it is one.
    pub fn player_position(&self, entity_id: u32) -> Option<usize> {
        self.player_pos.get(&entity_id).copied()
    }

    /// This entity's slot in `targets()`, if it is one.
    pub fn target_position(&self, entity_id: u32) -> Option<usize> {
        self.target_pos.get(&entity_id).copied()
    }
}
