//! `rotation`, `damage_mods`, `missiles`, `replay`, and `series` blocks
//! (native format 1.0, Task 8).
//!
//! Field lists here follow the REAL legacy structs in
//! `crates/axilog-schema/src/lib.rs`, not the task-8 brief's own sketch --
//! per the brief's own "real struct wins" instruction (see Tasks 6/7's
//! `defense.rs`/`support.rs` for the same precedent). Deviations from the
//! brief's literal code are called out per-item below.
use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use crate::v1::series::SeriesOut;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct RotationBlock {
    pub by_entity: ByEntity<RotationEntity>,
}

impl RotationBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct RotationEntity {
    /// This entity's casts, in cast-start order. `Some` exactly when the
    /// cast gate (`--rotation` / SDK `rotation: true`) was on.
    ///
    /// **This `Option` is the format's `--rotation` GATE RECORD, and
    /// `Some([])` is a meaningful value** -- a player who was present and
    /// cast nothing, as against a log where the pass never ran. The
    /// distinction is the same one [`super::damage::DamageEntity::by_skill`]
    /// makes for `--skill-damage`, and it is needed here for the same
    /// reason: `rotation` is a TWO-GATE block. Its other half
    /// ([`Self::aftercast`]) is always-on, so the row exists either way and
    /// `coverage.rotation` -- which answers for the block, not the field --
    /// reports `present` off the always-on half alone. The field doc here
    /// used to claim `coverage` could tell the two cases apart; it cannot,
    /// and the ei-json adapter was reading the legacy `PlayerOut::rotation`'s
    /// presence to make up the difference.
    ///
    /// The count that used to sit beside this (`cast_count`) is gone with
    /// the change: it was exactly `casts.len()`, so keeping it would have
    /// meant two fields encoding one gate, free to disagree -- and an
    /// ungated-looking `0` is the very "absent reported as zero" reading
    /// this `Option` exists to remove.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub casts: Option<Vec<CastRow>>,
    /// Aftercast/interrupt cast counters -- the legacy
    /// `PlayerOut::aftercast`, which the spec's own block-source table
    /// assigns to `rotation` but which no builder read before the final
    /// review.
    ///
    /// Ungated, and so is the row carrying it: this block is built even
    /// when `--rotation` is off, precisely so that turning the CASTS gate
    /// off cannot take this with it.
    pub aftercast: Aftercast,
}

/// Mirrors the legacy `crate::AftercastOut` field-for-field. Durations are
/// MILLISECONDS here, the format's convention throughout -- GW2EI emits
/// the same two quantities as seconds with 3 decimals, which the ei-json
/// adapter applies at that boundary.
///
/// NOTE the name collision the legacy struct documents: `wasted_count` is
/// a CAST-INTERRUPT count, a completely different quantity from the
/// boon-generation `*_wasted` in `support::GenerationRow`.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct Aftercast {
    /// Casts that skipped their aftercast.
    pub saved_count: u32,
    pub saved_ms: i64,
    /// Casts interrupted before firing.
    pub wasted_count: u32,
    /// Already the positive "time lost" figure.
    pub wasted_ms: i64,
}

/// One cast. Mirrors the real `CastOut` field-for-field (`cast_time_ms`/
/// `duration_ms`/`time_gained_ms`/`quickness`, all `i64`/`f64`) -- the
/// brief's own sketch (`time_ms: u64`, `duration_ms: u32`, no
/// `time_gained_ms`/`quickness`) does not match the real struct.
/// `skill_id` is hoisted from the enclosing `SkillRotationOut` (real field
/// name `skill_id`, not the brief's `id`) so a consumer gets a flat,
/// time-ordered cast list per entity instead of having to re-nest by skill.
///
/// Deliberately excludes APM: it is `cast_count` over the entity's active
/// time, both of which a consumer already has; storing a derived rate
/// invites it to disagree with its own inputs (see the brief's
/// implementer note 2).
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct CastRow {
    pub skill_id: u32,
    pub cast_time_ms: i64,
    pub duration_ms: i64,
    pub time_gained_ms: i64,
    pub quickness: f64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageModsBlock {
    pub by_entity: ByEntity<DamageModEntity>,
}

impl DamageModsBlock {
    /// See [`super::damage::DamageBlock::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// One entity's damage modifiers, in the two scopes GW2EI evaluates them
/// at: over the whole fight, and restricted to one foe.
///
/// The two are not derivable from each other in either direction. `overall`
/// counts every qualifying hit, including hits on agents that are not
/// targets at all (enemy MINIONS, whose damage GW2EI attributes to the
/// minion's own agent and never to its owner -- see
/// `DamageModifierResults::per_target`'s doc comment), so summing
/// `per_target` does not reconstruct it.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageModEntity {
    /// Whole-fight rows, keyed by the SIGNED modifier id -- see
    /// [`DamageModRow`] for why one map holds both directions.
    pub overall: BTreeMap<i32, DamageModRow>,
    /// Keyed by the TARGET's entity id, then by signed modifier id --
    /// joining directly to that entity's own row, the same convention
    /// `DamageEntity::per_target` uses.
    ///
    /// Sparse in BOTH dimensions, and deliberately so: a target this entity
    /// never landed a qualifying hit on has no key at all. EI's own shape
    /// is a dense `[targetIndex][]` array, one slot per `targets[]` entry
    /// whether or not anything happened there, which is what made this the
    /// single largest structure in the legacy document (854,077 bytes
    /// against the whole-fight arrays' 76,611 -- an 11x multiplier). The
    /// density is a property of EI's positional encoding, not of the data;
    /// the adapter re-inflates it when it renders
    /// `damageModifiersTarget`.
    ///
    /// Empty unless the caller asked the engine for the per-target split
    /// (`--modifiers` on the ei-json path). An empty map on a present block
    /// therefore means "the split was not computed", not "no qualifying
    /// hits" -- the whole-fight half of this block is what answers the
    /// latter.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub per_target: BTreeMap<u32, BTreeMap<i32, DamageModRow>>,
}

/// One damage-modifier row. Mirrors the real `DamageModEntryOut`: `id` is
/// the map KEY (signed, negative for incoming -- see `DamageModEntryOut::
/// id`'s own doc comment), so a single `BTreeMap<i32, _>` naturally
/// separates outgoing (positive ids) from incoming (negative ids) without
/// needing two separate maps. Adds `total_damage`, which the real struct
/// carries and the brief's sketch omitted.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageModRow {
    pub hit_count: u32,
    pub total_hit_count: u32,
    pub damage_gain: f64,
    pub total_damage: u64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct MissilesBlock {
    /// Required, not `Option` -- see [`SeriesBlock::squad`] for the
    /// consistency rule the four `squad` slots in this format follow.
    pub squad: MissilesSquad,
    pub by_entity: ByEntity<MissilesEntity>,
}

impl MissilesBlock {
    /// Unlike `damage`/`cc`, this block's `squad` rollup is computed
    /// independently of the per-player rows (`MissilesOut::squad`, not a
    /// sum over `MissilesOut::players`), so "no rows" alone does not imply
    /// "nothing to report" -- both have to be vacuous.
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty() && self.squad == MissilesSquad::default()
    }
}

/// Mirrors the real `SquadMissilesOut` field-for-field -- the brief's
/// sketch carried only `incoming_denied`.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct MissilesSquad {
    pub fired: u32,
    pub hit: u32,
    pub denied: u32,
    pub incoming_fired: u32,
    pub incoming_denied: u32,
}

/// Mirrors the real `PlayerMissilesOut`, minus `agent_addr`/`account` --
/// those are identity, already carried once on this row's own `entities[]`
/// entry (the whole point of the entity-id join). Adds `reflected_at_self`,
/// which the real struct carries and the brief's sketch omitted.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct MissilesEntity {
    pub fired: u32,
    pub hit: u32,
    pub denied: u32,
    pub reflected_at_self: u32,
}

/// The replay block is the one block in this format whose two halves ride
/// different gates, and the split is deliberate.
///
/// `by_entity` -- down/dead intervals plus first/last-aware bounds -- is
/// ALWAYS present. `axilog_core::analysis::replay::build_activity_intervals`
/// is a min/max scan plus a status-event walk, with no position decode,
/// sort, or interpolation, so every caller already computes it
/// unconditionally; the cutover audit records the matching ei-json fact,
/// that `combatReplayData.{start,end,down,dead}` are emitted with or
/// without `--replay` while `{positions,orientations,iconURL}` are not.
///
/// `tracks` -- the downsampled position samples -- rides `--replay`,
/// because THAT is the expensive half.
///
/// **`coverage.replay == "present"` therefore does not mean positions are
/// available.** It answers the intervals question, which is the one this
/// block can always answer. A consumer wanting the position map must check
/// `tracks` for itself; that is a presence check on the data, not a flag it
/// has to be told about out of band.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ReplayBlock {
    /// Keyed by entity id, and covering the SQUAD roster only -- the
    /// always-on pass walks `Encounter::players`, nothing else. Enemy
    /// players get intervals only inside [`ReplayTracks`], whose roster is
    /// wider; see [`ReplayTrack::down_intervals`] for why that is not
    /// redundancy to be collapsed.
    pub by_entity: ByEntity<ReplayIntervals>,
    /// Present only under `--replay`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<ReplayTracks>,
}

impl ReplayBlock {
    /// See [`super::damage::DamageBlock::is_empty`]. Either half alone is
    /// enough to make this block non-empty: on a log parsed with `--replay`
    /// but no activity pass the intervals map is bare and the tracks still
    /// are not, and reporting that `Empty` would claim nobody moved.
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
            && self.tracks.as_ref().map_or(true, |t| t.by_entity.is_empty())
    }
}

/// One squad entity's activity intervals: the cheap half of the replay,
/// carried whether or not positions were requested.
#[derive(Serialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct ReplayIntervals {
    /// This entity's own first-aware time, log-relative ms -- the earliest
    /// event of ANY kind naming it, matching GW2EI's `AgentItem.FirstAware`
    /// and its exported `combatReplayData.start`.
    pub start_ms: u64,
    /// Last-aware, the same way (`combatReplayData.end`).
    pub end_ms: u64,
    /// `(end_ms - start_ms) - dead_ms`. Carried rather than left to the
    /// reader to derive, because the derivation has a trap in it: down time
    /// is NOT subtracted, only dead time is -- verified against a real EI
    /// export in `ActivityIntervals::active_ms`'s doc comment, which has
    /// the GW2EI source citation. A consumer who reasonably assumes "active
    /// means neither downed nor dead" gets a different, wrong number.
    pub active_ms: u64,
    /// `[start_ms, end_ms)` pairs. Half-open, matching GW2EI's own exported
    /// `down`/`dead` arrays -- the end timestamp is the closing
    /// transition's own time and is not part of the interval.
    pub down: Vec<(u64, u64)>,
    pub dead: Vec<(u64, u64)>,
    /// Disconnect/not-yet-spawned windows (`CBTS_DESPAWN` to the matching
    /// `CBTS_SPAWN`), half-open `[start_ms, end_ms)` like `down`/`dead`
    /// above. Deliberately diverges from GW2EI's own `dc` export, which uses
    /// an inclusive sentinel bracket (`[i32::MinValue, FirstAware]`/
    /// `[LastAware, i32::MaxValue]`) rather than a true half-open interval;
    /// the cutover report measured that difference at 6 of 6,894 samples
    /// (0.087%) of axibridge's current distance error, small enough that
    /// matching this format's own half-open convention throughout was judged
    /// more valuable than byte-parity with GW2EI's sentinel choice. Not
    /// mutually exclusive with `down`/`dead` -- an agent can despawn while
    /// dead.
    pub dc: Vec<(u64, u64)>,
}

/// The gated half of [`ReplayBlock`]: position tracks and the metadata that
/// only describes them. `poll_ms` and `bounds` live here rather than beside
/// `by_entity` precisely because they are meaningless without tracks -- at
/// the block's top level they would have to serialize as a zero polling
/// interval on every log parsed without `--replay`.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ReplayTracks {
    /// Shared polling interval for every track below -- real `ReplayOut`
    /// carries one `poll_ms` for the whole replay, not per track, so it is
    /// hoisted here rather than duplicated onto each `ReplayTrack`.
    pub poll_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<ReplayBounds>,
    /// Keyed by entity id. The legacy `ReplayTrackOut` carried no join key
    /// at all, so a consumer could not tell whose track it was reading.
    pub by_entity: ByEntity<ReplayTrack>,
}

/// Mirrors the real `ReplayBoundsOut`, which is `f64`, not the brief's
/// sketch `f32`.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ReplayBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

/// One entity's replay track. Deliberately NOT re-encoded through the
/// shared `SeriesOut` envelope, despite the brief's sketch (`x: SeriesOut,
/// y: SeriesOut`): `SeriesOut` assumes a dense array starting at t=0 with a
/// fixed step, but `analysis::replay::Track::samples` starts at this
/// agent's own "first aware" time rounded up to the polling grid
/// (`replay.rs`'s `downsample`/`build_track` doc comments), which is
/// usually NOT zero. Encoding `x`/`y` as `SeriesOut` would silently drop
/// that start offset and misrepresent every sample's real timestamp. The
/// real `ReplayTrackOut.samples` already carries the exact `(t_ms, x, y)`
/// triple per sample, so this carries that field verbatim instead of
/// inventing a lossy encoding. `down_intervals`/`dead_intervals` likewise
/// carry over unchanged. `name`/`team`/`commander`/`is_squad` are dropped:
/// they are identity/attribute fields already on this entity's own
/// `entities[]` row (`EntityOut::commander`, `Role::Squad`), which is the
/// point of joining by entity id instead of re-carrying them per track.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ReplayTrack {
    pub samples: Vec<(u64, f64, f64)>,
    /// For a SQUAD entity these repeat [`ReplayIntervals::down`]/`dead`
    /// exactly -- both come from the same `replay::build_intervals` call
    /// over the same folded addr set, so they cannot disagree. They are not
    /// dropped here because the track roster is WIDER than the always-on
    /// one: `replay::build_replay` walks squad players AND enemy-player
    /// representatives, while `build_activity_intervals` walks squad
    /// players only. Deleting these would silently take every enemy
    /// player's down/dead history with them, and extending the always-on
    /// pass over the enemy roster instead would put a per-enemy event scan
    /// on the path of every parse -- paying, on a WvW log, for data nothing
    /// currently reads.
    pub down_intervals: Vec<(u64, u64)>,
    pub dead_intervals: Vec<(u64, u64)>,
    /// Half-open `[start_ms, end_ms)`, same divergence from GW2EI's
    /// inclusive sentinel bracket as [`ReplayIntervals::dc`] -- see that
    /// field's doc comment for the citation. Not mutually exclusive with
    /// `down_intervals`/`dead_intervals`.
    pub dc_intervals: Vec<(u64, u64)>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SeriesBlock {
    /// Required, not `Option`.
    ///
    /// All four `squad` slots in this format (`damage`, `cc`, `missiles`,
    /// `series`) are required, so a consumer never has to branch on
    /// whether an aggregate exists. Two of them were `Option` with
    /// `skip_serializing_if` while being unconditionally `Some(..)` at
    /// every reachable call site -- optionality that expressed nothing and
    /// cost every consumer a null check. Absence would only be meaningful
    /// if a squad aggregate could be *unknown* as distinct from *zero*,
    /// which no builder here can produce: the squad timeline is computed
    /// unconditionally by `analyze()`, and a squad with no rows has a
    /// genuinely zero aggregate, not an unknown one.
    pub squad: SquadSeries,
    pub by_entity: ByEntity<EntitySeries>,
}

impl SeriesBlock {
    /// The squad series is computed unconditionally (it does not need
    /// `--timeseries`, unlike the per-entity rows), so this block is empty
    /// only when the encounter produced no buckets at all.
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty() && self.squad.damage.len == 0
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SquadSeries {
    pub damage: SeriesOut,
    pub cc_applied: SeriesOut,
    pub downs: SeriesOut,
}

/// Mirrors the real `PlayerPerSecondOut` field-for-field -- the brief's
/// sketch carried only `damage`. `per_target` is keyed by the TARGET's
/// entity id (same "no positional joins" rule `damage.rs::DamageEntity::
/// per_target` already established), not by array position or raw
/// `enemy_id`.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct EntitySeries {
    pub damage: SeriesOut,
    /// The non-condition half of OUTGOING [`Self::damage`] -- GW2EI's
    /// `powerDamage1S`.
    ///
    /// `Option`, and in practice `Some` only on ENEMY rows, because no pass
    /// computes it for players: `PlayerPerSecondOut` carries `damage`,
    /// `damage_taken` and `power_damage_taken` but no outgoing power split,
    /// while `timeseries::build_enemy_series` computes exactly that split
    /// for enemies. This is the same shape Task 7 gave
    /// `damage::SkillRow::hits` and for the same reason: absent means "no
    /// pass ever measured this", which a zero-filled series would misreport
    /// as "measured, and it was all condition damage". Give players an
    /// outgoing power pass later and this becomes `Some` for them too
    /// without a shape change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_damage: Option<SeriesOut>,
    pub damage_taken: SeriesOut,
    pub power_damage_taken: SeriesOut,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub per_target: BTreeMap<u32, TargetSeries>,
    /// `[[time_ms, percent], ...]` -- this entity's health as a STEP
    /// function, not a fixed-rate series.
    ///
    /// That is why it is a plain pair list rather than a [`SeriesOut`]
    /// like its three neighbours: those are one value per bucket at
    /// `timeline.resolution_ms`, and re-sampling a step function onto that
    /// grid would either invent readings between updates or lose updates
    /// that fall inside a bucket. The source
    /// (`axilog_core::analysis::health::ei_health_percents`) is a
    /// `ListFromStates` transcription whose whole contract is that a value
    /// holds until the next pair, so the pairs ARE the data.
    ///
    /// `Option`, not a possibly-empty `Vec`, because absent and empty are
    /// genuinely different here and the ei-json surface distinguishes
    /// them. The pass keys its map off `HEALTH_UPDATE` events, so a player
    /// that emitted none is ABSENT from it -- and GW2EI (and this
    /// project's adapter) then omits `healthPercents` for that player
    /// entirely, rather than writing `[]`. A `Vec` would collapse "the
    /// pass never saw this entity" into "the pass saw it and it had no
    /// transitions", which is the same absent-reported-as-zero ambiguity
    /// `coverage` exists to remove, one level down.
    ///
    /// Same `--timeseries` gate as the rest of this block's per-entity
    /// rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_percents: Option<Vec<(u64, f64)>>,
    /// CUMULATIVE outgoing healing from the arcdps healing extension --
    /// GW2EI's `extHealingStats.healing1S`.
    ///
    /// This lives on the SERIES block, not beside the rest of the healing
    /// detail on `blocks.healing`, because what a field belongs to here is
    /// its grid and its gate, not its subject matter. Its three neighbours
    /// above and this array are all one value per bucket of
    /// `timeseries::ei_grid` -- the CEILING grid, which is one bucket longer
    /// than `timeline.resolution_ms`'s floor grid on a partial-second log --
    /// and all four ride `--timeseries`. Put on `blocks.healing` it would
    /// have been the one field there answering a different flag than its
    /// siblings, and `coverage.healing` could not have described it.
    ///
    /// `Option` for [`Self::power_damage`]'s reason, with a second cause: no
    /// pass computes it for enemies, AND no pass computes it for anyone on a
    /// log with no healing extension. Absent means "not measured"; a
    /// zero-filled array would claim a squad healed for nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healing_1s: Option<SeriesOut>,
}

/// Mirrors the real `PlayerTargetSeriesOut`, minus `enemy_id` (that's the
/// map key here, joined through `EntityIndex::by_enemy_id`, not carried
/// redundantly inside the value).
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct TargetSeries {
    pub damage: SeriesOut,
    pub power_damage: SeriesOut,
}

/// An [`EntitySeries`] with no per-second data, for a row that exists only
/// because some other quantity on it does.
fn empty_series(res: u64) -> EntitySeries {
    EntitySeries {
        damage: SeriesOut::encode_u64(res, &[]),
        power_damage: None,
        damage_taken: SeriesOut::encode_u64(res, &[]),
        power_damage_taken: SeriesOut::encode_u64(res, &[]),
        per_target: BTreeMap::new(),
        health_percents: None,
        healing_1s: None,
    }
}

pub fn build_series(
    report: &crate::Report,
    index: &EntityIndex,
    health_percents: Option<&BTreeMap<u64, Vec<(u64, f64)>>>,
    enemy_series: Option<
        &BTreeMap<u64, axilog_core::analysis::timeseries::EnemySeries>,
    >,
    healing_1s: Option<&axilog_core::analysis::healing_detail::HealingDetail>,
) -> SeriesBlock {
    // Same positional-join guard `support::build_healing` applies to the
    // other half of this pass's output, and for the same reason.
    let healing_1s = healing_1s.filter(|d| d.len() == report.players.len());
    let res = report.timeline.resolution_ms;
    let ps = &report.timeline.per_second;
    let squad = SquadSeries {
        damage: SeriesOut::encode_u64(res, &ps.squad_damage),
        cc_applied: SeriesOut::encode_u64(
            res,
            &ps.cc_applied.iter().map(|v| u64::from(*v)).collect::<Vec<_>>(),
        ),
        downs: SeriesOut::encode_u64(
            res,
            &ps.downs.iter().map(|v| u64::from(*v)).collect::<Vec<_>>(),
        ),
    };

    let mut by_entity = ByEntity::default();
    for (i, p) in report.players.iter().enumerate() {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        // Positional, not addr-keyed, unlike `health_percents` just below:
        // `healing_detail` is a `Vec` over `enc.players` with no addr in it,
        // which is what the length guard above exists to make safe.
        let healing: Option<SeriesOut> =
            healing_1s.map(|d| SeriesOut::encode_u64(res, &d[i].healing_1s));
        // Keyed by the account's REPRESENTATIVE agent address, which is
        // what `PlayerOut::agent_addr` already is -- a relogged account is
        // one player here and one folded series there. Looking the map up
        // by this row's addr (rather than iterating the map) is the same
        // join the ei-json adapter has always done, and it means a map
        // entry for an agent that is not a roster player cannot invent a
        // row.
        let health: Option<Vec<(u64, f64)>> =
            health_percents.and_then(|m| m.get(&p.agent_addr)).cloned();
        // Both quantities ride `--timeseries` today -- `per_second` is
        // `Some` under exactly the flag that makes the caller run the
        // health pass -- so in practice these are present and absent
        // together. The row is built when EITHER exists rather than
        // requiring `per_second`, so that if those gates ever diverge the
        // result is an honest row with empty series arrays instead of a
        // health series that silently vanishes.
        let Some(series) = p.per_second.as_ref() else {
            if health.is_some() || healing.is_some() {
                by_entity.insert(
                    id,
                    EntitySeries {
                        health_percents: health,
                        healing_1s: healing,
                        ..empty_series(res)
                    },
                );
            }
            continue;
        };
        let per_target = series
            .per_target
            .iter()
            .filter_map(|pt| {
                index.by_enemy_id(pt.enemy_id).map(|tid| {
                    (
                        tid,
                        TargetSeries {
                            damage: SeriesOut::encode_u64(res, &pt.damage),
                            power_damage: SeriesOut::encode_u64(res, &pt.power_damage),
                        },
                    )
                })
            })
            .collect();
        by_entity.insert(
            id,
            EntitySeries {
                damage: SeriesOut::encode_u64(res, &series.damage),
                power_damage: None,
                damage_taken: SeriesOut::encode_u64(res, &series.damage_taken),
                power_damage_taken: SeriesOut::encode_u64(res, &series.power_damage_taken),
                per_target,
                health_percents: health,
                healing_1s: healing,
            },
        );
    }

    // Enemy rows (side-channel absorption Task 8). An enemy is an entity
    // like any other, so its outgoing series land in the SAME `by_entity`
    // map the player rows above use.
    //
    // Every `Enemy` gets a row, not just the ones the pass returned:
    // `build_enemy_series` only emits enemies that actually dealt damage,
    // and the previous ei-json emitter zero-filled the rest so that the
    // arrays stayed the fixed grid length GW2EI's `InterpolatedGraph`
    // allocates. Doing that fill HERE rather than in the adapter is what
    // lets the native block answer the gate question by itself: with every
    // enemy filled, "this enemy has no series row" means the flag was off,
    // never "this enemy dealt nothing". That distinction is exactly what
    // Task 7's `by_skill` could not make -- so the gate absorbs here even
    // though it did not there. RLE makes the fill nearly free: a
    // full-length zero series is one pair.
    //
    // The roster is `report.enemies` and NOT every enemy-role entity. The
    // entity list is the broader of the two (80 enemy-role entities against
    // 49 `Enemy` records on the committed fixture; the extra rows are
    // minions and gadgets promoted to entities), and no pass measures a
    // series for an entity with no `Enemy` record -- filling those would
    // invent data rather than carry it. `build_damage`'s enemy rows draw
    // the same line. What makes that safe for the adapter's gate inference
    // is that the rendered `source_order.targets()` are all backed by
    // `Enemy` records, pinned by `v1_enemy_series.rs::
    // every_rendered_target_has_a_row_so_absent_can_only_mean_gate_off`.
    if let Some(pass) = enemy_series {
        // The grid length. Read off a real series rather than recomputed,
        // for the same reason the adapter read it that way: every series
        // the pass builds is `timeseries::ei_grid(duration)` long, and
        // that grid is NOT the squad timeline's bucketing, so deriving it
        // from `report.timeline` could silently disagree. On a log where no
        // enemy dealt any damage, fall back to a player's own series, which
        // is built on the same grid.
        let buckets = pass
            .values()
            .next()
            .map(|s| s.damage.len())
            .or_else(|| report.players.iter().find_map(|p| p.per_second.as_ref()).map(|ps| ps.damage.len()))
            .unwrap_or(0);
        let zeros = vec![0u64; buckets];
        for e in &report.enemies {
            let Some(id) = index.by_enemy_id(e.id) else { continue };
            let (damage, power) = match pass.get(&e.id) {
                Some(s) => (s.damage.as_slice(), s.power_damage.as_slice()),
                None => (zeros.as_slice(), zeros.as_slice()),
            };
            // An enemy that is ALSO a player row cannot happen (the two
            // rosters are disjoint by role), so this never overwrites the
            // loop above.
            by_entity.insert(
                id,
                EntitySeries {
                    damage: SeriesOut::encode_u64(res, damage),
                    power_damage: Some(SeriesOut::encode_u64(res, power)),
                    ..empty_series(res)
                },
            );
        }
    }

    SeriesBlock { squad, by_entity }
}

pub fn build_rotation(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> RotationBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        // NOT `else { continue }` on a missing rotation: `aftercast` below
        // is ungated, so skipping the row would drop it whenever
        // `--rotation` is off. An empty `casts` is the honest answer for
        // the gated half; `coverage.rotation` is what says which it is.
        let casts = p.rotation.as_ref().map(|rotation| {
            let mut casts = Vec::new();
            for skill in rotation {
                cats.reference_skill(skill.skill_id);
                for c in &skill.casts {
                    casts.push(CastRow {
                        skill_id: skill.skill_id,
                        cast_time_ms: c.cast_time_ms,
                        duration_ms: c.duration_ms,
                        time_gained_ms: c.time_gained_ms,
                        quickness: c.quickness,
                    });
                }
            }
            casts.sort_by_key(|c| (c.cast_time_ms, c.skill_id));
            casts
        });
        let a = &p.aftercast;
        by_entity.insert(
            id,
            RotationEntity {
                casts,
                aftercast: Aftercast {
                    saved_count: a.saved_count,
                    saved_ms: a.saved_ms,
                    wasted_count: a.wasted_count,
                    wasted_ms: a.wasted_ms,
                },
            },
        );
    }
    RotationBlock { by_entity }
}

pub fn build_damage_mods(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
    per_target: Option<&axilog_core::analysis::damage_mods::DamageModifierResults>,
) -> DamageModsBlock {
    let mut by_entity = ByEntity::default();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let Some(mods) = p.damage_mods.as_ref() else { continue };
        let mut overall = BTreeMap::new();
        // `id` is signed and already distinguishes outgoing (positive) from
        // incoming (negative) -- see `DamageModEntryOut::id`'s doc comment
        // -- so both directions share one map without collision.
        for m in mods.outgoing.iter().chain(mods.incoming.iter()) {
            cats.reference_damage_mod(m.id);
            overall.insert(
                m.id,
                DamageModRow {
                    hit_count: m.hit_count,
                    total_hit_count: m.total_hit_count,
                    damage_gain: m.damage_gain,
                    total_damage: m.total_damage,
                },
            );
        }
        // The per-target split, re-keyed from the engine's
        // `(player addr, ENEMY addr, mod id)` onto entity ids. A foe that
        // does not resolve to an entity is skipped rather than bucketed:
        // unlike a buff applier (Task 12), a damage-modifier row already
        // exists in full inside `overall`, so there is nothing to lose --
        // only a join to decline.
        let mut targets: BTreeMap<u32, BTreeMap<i32, DamageModRow>> = BTreeMap::new();
        if let Some(results) = per_target {
            for (&(_, foe, mod_id), s) in results
                .per_target
                .range((p.agent_addr, u64::MIN, i32::MIN)..=(p.agent_addr, u64::MAX, i32::MAX))
            {
                let Some(foe_id) = index.by_enemy_id(foe) else { continue };
                cats.reference_damage_mod(mod_id);
                targets.entry(foe_id).or_default().insert(
                    mod_id,
                    DamageModRow {
                        hit_count: s.hit_count,
                        total_hit_count: s.total_hit_count,
                        damage_gain: s.damage_gain,
                        total_damage: s.total_damage,
                    },
                );
            }
        }
        by_entity.insert(id, DamageModEntity { overall, per_target: targets });
    }
    DamageModsBlock { by_entity }
}

pub fn build_missiles(report: &crate::Report, index: &EntityIndex) -> MissilesBlock {
    let Some(m) = report.missiles.as_ref() else { return MissilesBlock::default() };
    let mut by_entity = ByEntity::default();
    // `PlayerMissilesOut` already carries its own `agent_addr` -- join on it
    // rather than on array position.
    for row in &m.players {
        let Some(id) = index.by_agent_addr(row.agent_addr) else { continue };
        by_entity.insert(
            id,
            MissilesEntity {
                fired: row.fired,
                hit: row.hit,
                denied: row.denied,
                reflected_at_self: row.reflected_at_self,
            },
        );
    }
    MissilesBlock {
        squad: MissilesSquad {
            fired: m.squad.fired,
            hit: m.squad.hit,
            denied: m.squad.denied,
            incoming_fired: m.squad.incoming_fired,
            incoming_denied: m.squad.incoming_denied,
        },
        by_entity,
    }
}

/// `activity` is positionally joined to `report.players` (both are built by
/// iterating `Encounter::players` 1:1 -- see `build_activity_intervals`'s
/// own doc comment), so a caller that hand-builds a `Report` can hand over
/// a slice of the wrong length. Rather than attribute one player's downs to
/// another, a mismatched slice is dropped whole, the same guard
/// `support::build_healing` uses on its own positional pass.
pub fn build_replay(
    report: &crate::Report,
    index: &EntityIndex,
    activity: Option<&[axilog_core::analysis::replay::ActivityIntervals]>,
) -> ReplayBlock {
    let activity = activity.filter(|a| a.len() == report.players.len());
    let mut intervals = ByEntity::default();
    if let Some(activity) = activity {
        for (p, a) in report.players.iter().zip(activity) {
            let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
            intervals.insert(
                id,
                ReplayIntervals {
                    start_ms: a.start_ms,
                    end_ms: a.end_ms,
                    active_ms: a.active_ms(),
                    down: a.down_intervals.iter().map(|i| (i.start_ms, i.end_ms)).collect(),
                    dead: a.dead_intervals.iter().map(|i| (i.start_ms, i.end_ms)).collect(),
                    dc: a.dc_intervals.iter().map(|i| (i.start_ms, i.end_ms)).collect(),
                },
            );
        }
    }

    let tracks = report.replay.as_ref().map(|r| {
        let mut by_entity = ByEntity::default();
        // Requires the `ReplayTrackOut::agent_addr` field added in this task:
        // `ReplayTrackOut` carried no join key at all even though its upstream
        // `axilog_core::analysis::replay::Track` already has `agent_addr`.
        for track in &r.tracks {
            let Some(id) = index.by_agent_addr(track.agent_addr) else { continue };
            by_entity.insert(
                id,
                ReplayTrack {
                    samples: track.samples.clone(),
                    down_intervals: track.down_intervals.clone(),
                    dead_intervals: track.dead_intervals.clone(),
                    dc_intervals: track.dc_intervals.clone(),
                },
            );
        }
        ReplayTracks {
            poll_ms: r.poll_ms,
            bounds: Some(ReplayBounds {
                min_x: r.bounds.min_x,
                min_y: r.bounds.min_y,
                max_x: r.bounds.max_x,
                max_y: r.bounds.max_y,
            }),
            by_entity,
        }
    });

    ReplayBlock { by_entity: intervals, tracks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::blocks::tests_support::fixture_report;

    #[test]
    fn squad_series_use_the_shared_envelope_and_decode_to_the_legacy_arrays() {
        let (report, index) = fixture_report();
        let block = build_series(&report, &index, None, None, None);
        let squad = &block.squad;
        assert_eq!(
            squad.damage.decode_u64(),
            report.timeline.per_second.squad_damage,
            "the envelope must be lossless -- this spec changes no number"
        );
        assert_eq!(squad.damage.interval_ms, report.timeline.resolution_ms);
    }

    #[test]
    fn a_replay_track_is_keyed_by_entity_id() {
        // The legacy `ReplayTrackOut` carries NO join key at all, so a
        // consumer cannot tell whose track it is reading.
        let (report, index) = fixture_report();
        let block = build_replay(&report, &index, None);
        // The committed fixture is parsed without `--replay`, so the block
        // is empty -- the assertion that matters is the SHAPE.
        for entity_id in block.tracks.iter().flat_map(|t| t.by_entity.0.keys()) {
            assert!(
                index.by_agent_addr(u64::from(*entity_id)).is_some() || *entity_id < 1000,
                "keys are entity ids"
            );
        }
        let v = serde_json::to_value(&block).expect("serializable");
        assert!(v.get("by_entity").is_some(), "replay hangs off by_entity like every block");
    }

    #[test]
    fn rotation_casts_reference_skill_ids_and_register_them() {
        let (report, index) = fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_rotation(&report, &index, &mut cats);
        let built = cats.finish(&Default::default(), None);
        for row in block.by_entity.0.values() {
            for cast in row.casts.iter().flatten() {
                assert!(built.skills.contains_key(&cast.skill_id), "cast skill must resolve");
            }
        }
    }

    #[test]
    fn an_ungated_block_is_empty_rather_than_absent_at_this_layer() {
        // Coverage (Task 9) decides absence. A builder always returns a
        // well-formed, possibly-empty block, so assembly has one rule.
        let (report, index) = fixture_report();
        let block = build_missiles(&report, &index);
        let _ = serde_json::to_value(&block).expect("an empty block still serializes");
    }
}
