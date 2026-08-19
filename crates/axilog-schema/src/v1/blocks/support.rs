use super::{ByEntity, PerSourceStates, StateTimeline};
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use serde::Serialize;
use std::collections::BTreeMap;

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct BoonsBlock {
    /// entity id -> buff id -> row. Two levels of real ids, no positional
    /// joins: the legacy shape was a `Vec` in `buffs::BOON_IDS` order, which
    /// a consumer could only read by knowing that table.
    pub by_entity: ByEntity<BTreeMap<u32, BoonRow>>,
}

impl BoonsBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Mirrors `crate::BoonOut` field-for-field, minus `id` (it's the map key)
/// and `name` (no human-readable name may appear in a block -- a consumer
/// resolves the id through the buff catalog instead).
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct BoonRow {
    pub uptime_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_stacks: Option<f64>,
    /// `crate::BoonOut::generation` is NOT optional in the legacy shape, so
    /// this stays unconditional too rather than inventing an `Option` the
    /// source data never has.
    pub generation: GenerationRow,
    /// This buff's fused stack timeline. `--timeseries` only.
    ///
    /// The two fields below are the reason `blocks.boons` is a two-gate
    /// block like `blocks.replay`: the uptime numbers above are computed on
    /// every parse, these are not. So `coverage.boons` answers the uptime
    /// question and is NOT a statement about whether the timelines are here
    /// -- check the fields.
    ///
    /// Duration boons are clamped to 0/1 upstream so the graph means what
    /// GW2EI's means (see `axilog_core::analysis::buffs::states`); intensity
    /// boons carry their real stack count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub states: Option<StateTimeline>,
    /// The same timeline split by applier. `--timeseries` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_source: Option<PerSourceStates>,
}

/// Mirrors `crate::GenerationOut` field-for-field, including the three
/// WASTED fields (rounded to 3 decimals upstream, omitted when exactly
/// zero -- same convention the legacy struct documents for the same
/// reason: the common case wastes nothing, and omission is real
/// information, not loss).
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct GenerationRow {
    pub self_pct: f64,
    pub group_pct: f64,
    pub squad_pct: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub self_wasted: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub group_wasted: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub squad_wasted: f64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SupportBlock {
    pub by_entity: ByEntity<SupportEntity>,
}

impl SupportBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Mirrors `crate::SupportOut` field-for-field, including
/// `strips_duration_ms` (the brief's sketch dropped it).
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SupportEntity {
    pub cleanses: u32,
    pub cleanses_self: u32,
    pub strips: u32,
    pub strips_duration_ms: u64,
    pub resurrects: u32,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ContributionBlock {
    pub by_entity: ByEntity<ContributionEntity>,
}

impl ContributionBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Both directions of the arcdps-methodology down contribution (M11).
/// GW2EI has no equivalent surface -- this follows arcdps itself.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ContributionEntity {
    pub downs_contribution: ContributionRow,
    pub downed_by: ContributionRow,
    /// [`Self::downs_contribution`]`.damage`, sliced by the skill that
    /// dealt it -- the legacy `PlayerOut::downs_contribution_per_skill`.
    ///
    /// The last piece of `players[]` data the ei-json adapter could not
    /// read from this document: it is `#[serde(skip)]` on the legacy struct,
    /// so it reached ei-json's `totalDamageDist[].downContribution` through
    /// the private side channel and no consumer of the native format could
    /// see it at all. The equivalence test recorded it as "intentionally
    /// absent from 1.0"; side-channel absorption Task 13 is what made that
    /// no longer tenable.
    ///
    /// Here rather than on `damage.by_entity[].by_skill` even though the
    /// only consumer joins it to those rows, because the gates differ: the
    /// per-skill damage rows ride `--skill-damage` while the contribution
    /// pass is always-on, so hanging this off them would make an ungated
    /// quantity vanish with a flag it has nothing to do with. Sparse --
    /// only skills with a nonzero credit appear, which is also exactly the
    /// condition GW2EI's own `int?` field is written under.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub downs_contribution_by_skill: BTreeMap<u32, u64>,
}

/// Mirrors `crate::ContributionOut` field-for-field. `movement_impairing`
/// is `u64` there (not `u32` as an earlier draft of this block assumed),
/// so this row keeps it `u64` too.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ContributionRow {
    pub damage: u64,
    pub cc: u32,
    pub strips: u32,
    pub movement_impairing: u64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HealingBlock {
    /// The addon's own registration descriptor -- GW2EI's `usedExtensions`
    /// entry minus the roster (which lives per-entity on
    /// [`HealingEntity::runs_extension`]). `None` only on a block that
    /// somehow exists without a registration; the block itself is omitted
    /// when the extension is absent, so in practice this is `Some`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<HealingExtensionDesc>,
    pub by_entity: ByEntity<HealingEntity>,
}

/// Mirrors GW2EI's `ExtensionDesc` minus `runningExtension`. `name` is
/// omitted: this descriptor hangs off the healing block, so the extension it
/// describes is never in question.
#[derive(Serialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct HealingExtensionDesc {
    /// The addon's self-reported version string, e.g. `"2.16rc1"`.
    /// `"Unknown"` when the registration row carries no decodable one --
    /// GW2EI's own default for the same field.
    pub version: String,
    pub revision: u32,
    pub signature: u32,
}

impl HealingBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Mirrors `crate::HealingOut` field-for-field. Field names/shape differ
/// substantially from the brief's sketch (`outgoing`/`outgoing_barrier`/
/// `downed_healing`) -- the real struct is `healing_out_total`/
/// `healing_out_allies`/`healing_out_self`/`barrier_out`/
/// `downed_healing_out`; all five are carried across.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct HealingEntity {
    pub outgoing_total: u64,
    pub outgoing_allies: u64,
    pub outgoing_self: u64,
    pub barrier_out: u64,
    pub downed_healing_out: u64,
    /// Whether this player's own arcdps healing-stats addon reported to the
    /// log -- GW2EI's `RunningExtension` roster, mirrored per-entity rather
    /// than as a separate name list because every other per-player fact in
    /// this format lives on its entity row.
    ///
    /// Consumers use this to say "these healing numbers are complete for
    /// this player" vs "partial, relayed by someone else's addon". A row
    /// with `outgoing_total > 0` and `runs_extension: false` is normal and
    /// expected -- see
    /// `axilog_core::analysis::healing::running_extension`.
    pub runs_extension: bool,
    /// The per-ally and per-skill breakdowns behind the five scalars above
    /// (`axilog_core::analysis::healing_detail`), when the `--skill-damage`
    /// gate ran that pass.
    ///
    /// One `Option` around all three maps rather than three siblings, for
    /// [`super::damage::PerTargetDetail`]'s reason: they are filled by a
    /// single pass over a single event list, so their presence is genuinely
    /// all-or-nothing and three independent `Option`s would let a
    /// consumer's type-checker accept a state no builder can produce.
    ///
    /// The 1S healing graph is NOT here -- see
    /// [`super::activity::EntitySeries::healing_1s`] for where it went and
    /// why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<HealingDetailCols>,
}

/// The three per-ally / per-skill breakdowns of one player's outgoing
/// healing and barrier.
#[derive(Serialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct HealingDetailCols {
    /// Healing and barrier this player put on each ally, keyed by the
    /// ALLY's entity id -- GW2EI's `outgoingHealingAllies` /
    /// `outgoingBarrierAllies`, which are positional arrays over
    /// `log.Friendlies` (this project's `enc.players`).
    ///
    /// Keyed, not positional, for the reason this format keys everything:
    /// the source array is dense and square, and a reader who miscounts its
    /// offset silently attributes one player's healing to another rather
    /// than failing. It also makes the payload sparse -- the two EI arrays
    /// are N*N cells of which a real squad fills a small fraction, and a
    /// cell that is zero in all three quantities is omitted here. Within a
    /// present map, an absent ally is a MEASURED zero (this player healed
    /// them for nothing); the `Option` one level up is what carries "not
    /// measured".
    ///
    /// The healer appears at its own id -- self-healing is one of these
    /// cells, exactly as in GW2EI, not a separate scalar.
    pub by_ally: BTreeMap<u32, AllyHealingRow>,
    /// `totalHealingDist`, keyed by skill id.
    pub by_skill: BTreeMap<u32, HealSkillRow>,
    /// `totalBarrierDist`, keyed by skill id. A separate map rather than a
    /// column on [`Self::by_skill`]: a skill can appear in one and not the
    /// other, and merging them would force every healing row to publish a
    /// zero barrier it never measured.
    pub barrier_by_skill: BTreeMap<u32, HealSkillRow>,
}

/// One cell of [`HealingDetailCols::by_ally`].
#[derive(Serialize, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AllyHealingRow {
    pub healing: u64,
    /// The subset of [`Self::healing`] that landed while the ally was downed
    /// (`EXTHealingEvent.AgainstDowned`).
    pub downed_healing: u64,
    /// EI's `outgoingBarrierAllies[..].barrier`. Folded into this row rather
    /// than kept as a second parallel map: it is the same pass, the same
    /// indexing and the same event list, so two maps would be two key sets
    /// with nothing forcing them to agree.
    pub barrier: u64,
}

impl AllyHealingRow {
    /// A cell with nothing in it at all -- omitted from `by_ally` rather
    /// than stored, which is what makes the N*N matrix sparse.
    fn is_empty(&self) -> bool {
        self.healing == 0 && self.downed_healing == 0 && self.barrier == 0
    }
}

/// One skill's row of `totalHealingDist` / `totalBarrierDist`.
///
/// `hits`/`min`/`max` count EVERY event in the group. GW2EI's healing dist
/// has no `HasHit` gate (`BuildHealingDist` accumulates unconditionally),
/// unlike its damage dist -- which is why this row has none of the three
/// hit counts [`super::damage::SkillRow`] carries, just the one.
#[derive(Serialize, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HealSkillRow {
    pub total: u64,
    /// EI's `totalDownedHealing`. Omitted when zero, which is ALWAYS on a
    /// barrier row: `EXTJsonBarrierDist` has no downed field at all, so
    /// emitting a zero there would invent a measurement GW2EI does not
    /// make.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub total_downed: u64,
    pub hits: u32,
    pub min: u64,
    pub max: u64,
    /// The group contains at least one `EXTNonDirectHealingEvent` (a
    /// healing-over-time tick) -- EI's `indirectHealing` /
    /// `indirectBarrier`.
    pub indirect: bool,
}

pub fn build_boons(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> BoonsBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let mut rows = BTreeMap::new();
        for b in &p.boons {
            cats.reference_buff(b.id);
            rows.insert(
                b.id,
                BoonRow {
                    uptime_pct: b.presence_pct,
                    avg_stacks: b.avg_stacks,
                    generation: GenerationRow {
                        self_pct: b.generation.self_pct,
                        group_pct: b.generation.group_pct,
                        squad_pct: b.generation.squad_pct,
                        self_wasted: b.generation.self_wasted,
                        group_wasted: b.generation.group_wasted,
                        squad_wasted: b.generation.squad_wasted,
                    },
                    // Filled by `attach_boon_states` when `--timeseries`
                    // supplied the pass; this builder is the always-on half.
                    states: None,
                    per_source: None,
                },
            );
        }
        by_entity.insert(id, rows);
    }
    BoonsBlock { by_entity }
}

/// Fold one applier's timeline into a [`PerSourceStates`], routing it to
/// either the resolved side or the `unresolved` bucket.
///
/// Both callers need the same merge-don't-overwrite rule, for the same
/// reason: the map being folded is keyed by AGENT ADDRESS, and several
/// addresses can land on one destination -- every relog address of a player
/// folds onto that player's single entity id, and every applier that
/// resolves to nothing at all folds onto the one `unresolved` bucket.
pub(super) fn merge_source_timeline(
    out: &mut PerSourceStates,
    source_entity: Option<u32>,
    timeline: &StateTimeline,
) {
    use axilog_core::analysis::buffs::states::merge_step_timelines;
    match source_entity {
        Some(sid) => match out.by_source.entry(sid) {
            std::collections::btree_map::Entry::Vacant(v) => {
                v.insert(timeline.clone());
            }
            std::collections::btree_map::Entry::Occupied(mut o) => {
                let merged = merge_step_timelines(o.get(), timeline);
                o.insert(merged);
            }
        },
        None => {
            out.unresolved = Some(match out.unresolved.take() {
                Some(prev) => merge_step_timelines(&prev, timeline),
                None => timeline.clone(),
            });
        }
    }
}

/// Attach the `--timeseries`-gated stack timelines to the boon rows.
///
/// This runs AFTER [`build_boons`] and only enriches rows that pass already
/// made: the uptime pass enumerates every tracked boon for every roster
/// player, so a `(player, boon)` with a timeline but no row would mean the
/// two passes disagreed about the roster. Such a pair is skipped rather than
/// conjured into a row with fabricated zero uptime and zero generation,
/// which is what `or_default()` insertion would have written.
pub fn attach_boon_states(
    block: &mut BoonsBlock,
    index: &EntityIndex,
    states: &axilog_core::analysis::buffs::BoonStates,
) {
    // Every row gets a `states`, including `Some([])` for a boon this
    // player never held. That empty vec is load-bearing in two ways: it is
    // the only thing distinguishing "the timeline pass ran and found
    // nothing" from "the pass did not run" (a real timeline always carries
    // at least the leading `[0, 0]` pair, so `[]` is unambiguous), and it is
    // what lets a consumer -- the ei-json adapter included -- use
    // `states.is_some()` as the gate signal instead of needing a separate
    // flag threaded alongside the data.
    for rows in block.by_entity.0.values_mut() {
        for row in rows.values_mut() {
            row.states = Some(Vec::new());
        }
    }
    for (&(addr, buff_id), timeline) in &states.total {
        let Some(entity_id) = index.by_agent_addr(addr) else { continue };
        let Some(row) = block.by_entity.0.get_mut(&entity_id).and_then(|m| m.get_mut(&buff_id))
        else {
            continue;
        };
        row.states = Some(timeline.clone());
    }
    for (&(addr, buff_id), per_source) in &states.per_source {
        let Some(entity_id) = index.by_agent_addr(addr) else { continue };
        let Some(row) = block.by_entity.0.get_mut(&entity_id).and_then(|m| m.get_mut(&buff_id))
        else {
            continue;
        };
        let mut folded = PerSourceStates::default();
        for (&source_addr, timeline) in per_source {
            merge_source_timeline(&mut folded, index.by_agent_addr(source_addr), timeline);
        }
        if !folded.is_empty() {
            row.per_source = Some(folded);
        }
    }
}

pub fn build_support(report: &crate::Report, index: &EntityIndex) -> SupportBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        by_entity.insert(
            id,
            SupportEntity {
                cleanses: p.support.cleanses,
                cleanses_self: p.support.cleanses_self,
                strips: p.support.strips,
                strips_duration_ms: p.support.strips_duration_ms,
                resurrects: p.support.resurrects,
            },
        );
    }
    SupportBlock { by_entity }
}

pub fn build_contribution(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> ContributionBlock {
    let row = |c: &crate::ContributionOut| ContributionRow {
        damage: c.damage,
        cc: c.cc,
        strips: c.strips,
        movement_impairing: c.movement_impairing,
    };
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        by_entity.insert(
            id,
            ContributionEntity {
                downs_contribution: row(&p.downs_contribution),
                downed_by: row(&p.downed_by),
                downs_contribution_by_skill: p
                    .downs_contribution_per_skill
                    .iter()
                    .filter(|(_, &v)| v > 0)
                    .map(|(&skill, &v)| {
                        cats.reference_skill(skill);
                        (skill, v)
                    })
                    .collect(),
            },
        );
    }
    ContributionBlock { by_entity }
}

/// `crate::PlayerOut::healing` is `Option<HealingOut>` -- `None` when the
/// log carries no healing-extension data at all
/// (`Metrics::has_healing_extension == false`), a real "no data" signal,
/// not "genuinely all zero". A player without healing data gets no row
/// here, same "absent, not null/zero" convention the legacy field itself
/// uses.
/// `detail` is `axilog_core::analysis::healing_detail::build`'s output when
/// the `--skill-damage` gate ran it -- positionally joined to
/// `report.players`, and dropped whole (see [`positional`]) if that join
/// cannot be trusted.
pub fn build_healing(
    report: &crate::Report,
    index: &EntityIndex,
    detail: Option<&axilog_core::analysis::healing_detail::HealingDetail>,
    cats: &mut CatalogBuilder,
    extension: Option<&axilog_core::evtc::ext_healing::Registration>,
) -> HealingBlock {
    let detail = positional(report, detail);
    // Entity id per `report.players` position -- the join `detail`'s ally
    // arrays need, resolved once instead of per cell.
    let ally_ids: Vec<Option<u32>> =
        report.players.iter().map(|p| index.by_agent_addr(p.agent_addr)).collect();

    let mut by_entity = ByEntity::default();
    for (i, p) in report.players.iter().enumerate() {
        let Some(id) = ally_ids[i] else { continue };
        let Some(h) = p.healing.as_ref() else { continue };
        by_entity.insert(
            id,
            HealingEntity {
                outgoing_total: h.healing_out_total,
                outgoing_allies: h.healing_out_allies,
                outgoing_self: h.healing_out_self,
                barrier_out: h.barrier_out,
                downed_healing_out: h.downed_healing_out,
                runs_extension: h.runs_extension,
                detail: detail.map(|d| build_healing_detail(&d[i], &ally_ids, cats)),
            },
        );
    }
    HealingBlock {
        extension: extension.map(|r| HealingExtensionDesc {
            version: r.version.clone(),
            revision: r.revision,
            signature: r.signature,
        }),
        by_entity,
    }
}

/// The positional-join guard the ally matrix needs.
///
/// `healing_detail`'s arrays are indexed by `enc.players` position, and
/// `report.players` is built from that same list in that same order -- but a
/// hand-built `Report` (every unit test that constructs one) can violate it.
/// Mis-attributing one player's healing to another is worse than omitting
/// the breakdown, so a length mismatch drops the whole surface rather than
/// emitting a shifted one. Same guard, same reason, as the ei-json adapter's
/// own `replay` filter.
fn positional<'a>(
    report: &crate::Report,
    detail: Option<&'a axilog_core::analysis::healing_detail::HealingDetail>,
) -> Option<&'a axilog_core::analysis::healing_detail::HealingDetail> {
    detail.filter(|d| d.len() == report.players.len())
}

fn build_healing_detail(
    d: &axilog_core::analysis::healing_detail::PlayerHealingDetail,
    ally_ids: &[Option<u32>],
    cats: &mut CatalogBuilder,
) -> HealingDetailCols {
    let mut by_ally: BTreeMap<u32, AllyHealingRow> = BTreeMap::new();
    for (i, &ally) in ally_ids.iter().enumerate() {
        let Some(ally) = ally else { continue };
        let row = AllyHealingRow {
            healing: d.ally_healing[i].healing,
            downed_healing: d.ally_healing[i].downed_healing,
            barrier: d.ally_barrier[i],
        };
        if !row.is_empty() {
            by_ally.insert(ally, row);
        }
    }

    let mut dist = |src: &[axilog_core::analysis::healing_detail::HealDistEntry]| {
        src.iter()
            .map(|e| {
                // Every id this block joins on has to resolve in the
                // catalog, or the row is a dangling reference -- the same
                // hole Task 9 found on the damage side.
                cats.reference_skill(e.skill_id);
                (
                    e.skill_id,
                    HealSkillRow {
                        total: e.total,
                        total_downed: e.total_downed,
                        hits: e.hits,
                        min: e.min,
                        max: e.max,
                        indirect: e.indirect,
                    },
                )
            })
            .collect()
    };
    HealingDetailCols {
        by_ally,
        by_skill: dist(&d.healing_dist),
        barrier_by_skill: dist(&d.barrier_dist),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::blocks::tests_support::fixture_report;
    use crate::v1::catalogs::CatalogBuilder;

    /// Both collapse paths in `merge_source_timeline`, neither of which the
    /// committed fixture exercises: it has no relogged applier and no
    /// applier that fails to resolve, so `blocks.*.per_source.by_source`
    /// never collides and `unresolved` is never written. Without this test
    /// those two branches would ship unexercised.
    #[test]
    fn appliers_that_land_on_one_destination_merge_rather_than_overwrite() {
        // Two addresses of one relogged player -> one entity id. Held
        // 0..1000 in the first session and 2000..3000 in the second; the
        // merge is a pointwise SUM, so the result must show both windows
        // rather than only the last one written.
        let mut resolved = PerSourceStates::default();
        merge_source_timeline(&mut resolved, Some(7), &vec![(0, 0), (0, 1), (1_000, 0)]);
        merge_source_timeline(&mut resolved, Some(7), &vec![(0, 0), (2_000, 1), (3_000, 0)]);
        assert_eq!(
            resolved.by_source.get(&7).map(Vec::as_slice),
            Some(&[(0, 1), (1_000, 0), (2_000, 1), (3_000, 0)][..]),
            "a relogged applier's two sessions must both survive"
        );
        assert!(resolved.unresolved.is_none(), "both appliers resolved");

        // Two appliers that resolve to nothing fold onto the one bucket,
        // and overlapping windows there SUM rather than clamp -- the bucket
        // is a count of concurrent applications, not a presence flag.
        let mut unresolved = PerSourceStates::default();
        merge_source_timeline(&mut unresolved, None, &vec![(0, 0), (0, 1), (2_000, 0)]);
        merge_source_timeline(&mut unresolved, None, &vec![(0, 0), (1_000, 1), (3_000, 0)]);
        assert!(unresolved.by_source.is_empty(), "nothing resolved to an entity");
        assert_eq!(
            unresolved.unresolved.as_deref(),
            Some(&[(0, 1), (1_000, 2), (2_000, 1), (3_000, 0)][..]),
            "the unresolved bucket must accumulate every applier that lands in it"
        );
        assert!(!unresolved.is_empty(), "a bucket-only row is not an empty row");
    }

    #[test]
    fn boons_are_keyed_by_buff_id_not_by_position_in_a_fixed_array() {
        // The legacy shape is `Vec<BoonOut>` in `buffs::BOON_IDS` order --
        // a positional join a consumer must know the table to read.
        let (report, index) = fixture_report();
        let mut cats = CatalogBuilder::default();
        let block = build_boons(&report, &index, &mut cats);

        let p = &report.players[0];
        let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
        let row = block.by_entity.get(id).expect("boon row");
        assert!(!row.is_empty(), "player carries per-boon rows");
        for buff_id in row.keys() {
            assert!(*buff_id > 0, "keys are real buff ids");
        }
    }

    #[test]
    fn every_referenced_buff_id_resolves_in_the_catalog() {
        let (report, index) = fixture_report();
        let mut cats = CatalogBuilder::default();
        let block = build_boons(&report, &index, &mut cats);
        let built = cats.finish(&Default::default(), None);
        for row in block.by_entity.0.values() {
            for buff_id in row.keys() {
                assert!(built.buffs.contains_key(buff_id), "buff {buff_id} must resolve");
            }
        }
    }

    #[test]
    fn boon_uptime_matches_the_legacy_report_exactly() {
        let (report, index) = fixture_report();
        let mut cats = CatalogBuilder::default();
        let block = build_boons(&report, &index, &mut cats);
        for p in &report.players {
            let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
            let row = block.by_entity.get(id).expect("row");
            for legacy in &p.boons {
                let got = row.get(&legacy.id).expect("boon present");
                assert_eq!(got.uptime_pct, legacy.presence_pct, "no number may change");
            }
        }
    }

    #[test]
    fn contribution_carries_both_directions() {
        let (report, index) = fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_contribution(&report, &index, &mut cats);
        let p = &report.players[0];
        let id = index.by_agent_addr(p.agent_addr).expect("player resolves");
        let row = block.by_entity.get(id).expect("row");
        assert_eq!(row.downs_contribution.damage, p.downs_contribution.damage);
        assert_eq!(row.downed_by.damage, p.downed_by.damage);
        // The per-skill slice carries only nonzero credits, and every skill
        // it names must be resolvable through the catalog it registered in.
        let built = cats.finish(&Default::default(), None);
        for (skill, credit) in &row.downs_contribution_by_skill {
            assert!(*credit > 0, "only nonzero credits are carried");
            assert!(built.skills.contains_key(skill), "down-contribution skill must resolve");
        }
    }
}
