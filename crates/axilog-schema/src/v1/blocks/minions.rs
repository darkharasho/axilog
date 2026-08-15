//! `minions` -- per-player minion damage-TAKEN rollups.
//!
//! The pass (`axilog_core::analysis::minions`) is gated on `--skill-damage`
//! and joined POSITIONALLY to `enc.players`. This block is the entity-keyed
//! reprojection of it, following the same rule the rest of the container
//! does: no positional joins survive onto the native wire, because a
//! consumer that has to know the encounter's player order to read a block
//! is one roster change away from silently reading the wrong row.
use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::order::SourceOrder;
use axilog_core::analysis::minions::MinionRollups;
use serde::Serialize;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct MinionsBlock {
    /// One entry per player that HAS minions. A player with none is absent
    /// rather than carrying an empty vec -- the pass itself distinguishes
    /// the two (it allocates an empty vec per player), and an absent key
    /// and an empty list mean the same thing to a reader while the absent
    /// form costs nothing on the wire. This is also what the ei-json
    /// adapter already did with `minions[]`, so the block matches the
    /// surface it feeds.
    pub by_entity: ByEntity<Vec<MinionRow>>,
}

impl MinionsBlock {
    /// No player had minions. Unlike `missiles`/`series` this block carries
    /// no independently computed aggregate, so "no rows" is the whole
    /// story.
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// One minion group belonging to one player.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct MinionRow {
    /// Resolves through `catalogs.minions`, which carries this group's
    /// species id and name. Identity lives there rather than on this row
    /// because no block in this format inlines a human-readable name.
    pub minion_id: u32,
    /// The damage this species took, by skill id. A map, not the pass's
    /// sorted vec: every other per-skill family in this container is a
    /// `BTreeMap<u32, _>` keyed by skill id, and a `BTreeMap` iterates in
    /// that same ascending-id order, so the wire shape gains a lookup key
    /// without changing the order anything sees. Keying cannot collapse
    /// rows: the pass accumulates into a `BTreeMap<u32, _>` keyed by skill
    /// id and only flattens to a vec on the way out, so ids are already
    /// unique within a group.
    pub taken: std::collections::BTreeMap<u32, MinionSkillTakenRow>,
}

/// One minion's damage-taken row for one skill.
///
/// Deliberately NOT `damage::SkillRow`. That row is the reprojection of
/// `crate::SkillEntryOut` and carries `crit_hits`/`flank_hits`, which a
/// damage-taken rollup does not have; this one carries the eight outcome
/// counters and `indirect`, which `SkillRow` does not have yet. Task 9
/// adds outcome columns to `SkillRow` from a different pass
/// (`dist_outcomes`), and the two may be worth converging afterwards --
/// but merging them NOW would mean one struct where each half of the
/// fields is meaningless for half the rows, which is the ambiguity
/// `coverage` exists to prevent, one level down.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct MinionSkillTakenRow {
    pub total: u64,
    /// Attempts, marker rows excluded.
    pub hits: u32,
    /// `HasHit` rows only.
    pub connected_hits: u32,
    pub min: u64,
    pub max: u64,
    pub blocked: u32,
    pub evaded: u32,
    pub glance: u32,
    /// `IsBlind`.
    pub missed: u32,
    /// `IsAbsorbed`.
    pub invulned: u32,
    pub interrupted: u32,
    /// Condition damage rather than strike damage.
    pub indirect: bool,
}

pub fn build_minions(
    rollups: &MinionRollups,
    order: &SourceOrder,
    cats: &mut CatalogBuilder,
) -> MinionsBlock {
    let mut by_entity = ByEntity::default();
    let players = order.players();
    for (i, groups) in rollups.iter().enumerate() {
        if groups.is_empty() {
            continue;
        }
        // The one place the positional join is consumed. A pass longer
        // than the roster it was built from cannot be positionally joined
        // to anything, so its extra slots are dropped rather than guessed
        // at -- the same length-invariant discipline `EiInputs` applied
        // before the adapter would touch any of these.
        let Some(&entity_id) = players.get(i) else { continue };
        let rows: Vec<MinionRow> = groups
            .iter()
            .map(|g| MinionRow {
                minion_id: cats.reference_minion(g.species_id, &g.name),
                taken: g
                    .taken
                    .iter()
                    .map(|r| {
                        cats.reference_skill(r.skill_id);
                        (
                            r.skill_id,
                            MinionSkillTakenRow {
                                total: r.total,
                                hits: r.hits,
                                connected_hits: r.connected_hits,
                                min: r.min,
                                max: r.max,
                                blocked: r.blocked,
                                evaded: r.evaded,
                                glance: r.glance,
                                missed: r.missed,
                                invulned: r.invulned,
                                interrupted: r.interrupted,
                                indirect: r.indirect,
                            },
                        )
                    })
                    .collect(),
            })
            .collect();
        by_entity.insert(entity_id, rows);
    }
    MinionsBlock { by_entity }
}
