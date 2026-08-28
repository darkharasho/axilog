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
use std::collections::{BTreeMap, BTreeSet};

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
    /// SPEC name -> the signed ids in [`crate::v1::catalogs::Catalogs::
    /// damage_mods`] that belong to that spec rather than to the shared
    /// pool -- EI's top-level `personalDamageMods`, which the ei-json
    /// adapter renders straight from here.
    ///
    /// Filtered to ids this block actually referenced, so the "every id
    /// here is a catalog key" invariant holds even when an entity fails to
    /// join the roster index. Empty means UNCLASSIFIED -- see
    /// `axilog_core::analysis::damage_mods::DamageModifierResults::
    /// personal`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub personal: BTreeMap<String, Vec<i32>>,
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
    /// Glider deploy/stow windows -- see
    /// [`axilog_core::analysis::agent_states`], including why this data
    /// exists in axilog and not in GW2EI's output.
    ///
    /// A flat list rather than a `ByEntity` map, and that is not an
    /// oversight: `CBTS_GLIDER` is not restricted to the squad, so a glider
    /// window can belong to an agent that never becomes a tracked entity.
    /// This follows `MarkerAssignmentOut`'s precedent exactly -- always
    /// carry `agent_addr`, carry `entity_id` only when the join resolves --
    /// because the alternative is silently dropping every non-roster row.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gliding: Vec<GliderOut>,
    /// Transformation (mount/tonic/form) windows. Same flat-list rationale
    /// as [`Self::gliding`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub transformations: Vec<TransformationOut>,
    /// Capture-point areas as DECODED: who held each point, who was taking
    /// it, and the progress timeline. See [`Self::decorations`] for why both
    /// forms are carried.
    ///
    /// Empty on every log written before arcdps build `20260602`, which does
    /// not emit the family at all.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub captures: Vec<CaptureOut>,
    /// Capture-point areas as RENDERABLE environment decorations -- the port
    /// of GW2EI's only consumer of this family
    /// (`LogLogic.ComputeEnvironmentCombatReplayDecorations`).
    ///
    /// Derived from [`Self::captures`], and carried alongside it rather than
    /// instead of it: see
    /// [`axilog_core::analysis::replay_extras::ReplayExtras::decorations`]
    /// for why neither form reconstructs the other.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub decorations: Vec<DecorationOut>,
}

impl ReplayBlock {
    /// See [`super::damage::DamageBlock::is_empty`]. Any one half alone is
    /// enough to make this block non-empty: on a log parsed with `--replay`
    /// but no activity pass the intervals map is bare and the tracks still
    /// are not, and reporting that `Empty` would claim nobody moved. The
    /// same argument extends to the eye-candy families, which are populated
    /// independently of both gates.
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
            && self.tracks.as_ref().map_or(true, |t| t.by_entity.is_empty())
            && self.gliding.is_empty()
            && self.transformations.is_empty()
            && self.captures.is_empty()
            && self.decorations.is_empty()
    }
}

/// One glider deployment. `end_ms` absent means the glider was still
/// deployed at the last event in the log -- NOT that it closed at log end.
/// See [`axilog_core::analysis::agent_states::GliderInterval::end_ms`].
#[derive(Serialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct GliderOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<u32>,
    pub agent_addr: u64,
    pub start_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
}

/// One transformation window.
#[derive(Serialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct TransformationOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<u32>,
    pub agent_addr: u64,
    /// The arcdps SESSION-LOCAL id. Meaningless across logs on its own --
    /// [`Self::guid`] is the portable identity, and is absent when the log
    /// carried no `CBTS_IDTOGUID` mapping for this id.
    pub transformation_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    pub start_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
}

/// One capture-point area over its lifetime.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct CaptureOut {
    /// The capture gadget. Almost never resolves to a tracked entity -- a
    /// capture point is a gadget, and `wvw::apply` retains only enemies that
    /// took a hostile hit -- but the slot is carried for the same reason
    /// [`GliderOut::entity_id`] is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<u32>,
    pub agent_addr: u64,
    /// Log-relative ms. Signed throughout this family; see
    /// [`DecorationOut::start_ms`].
    pub start_ms: i64,
    /// Absent when the area never got a hide row and no later show
    /// superseded it. Deliberately NOT defaulted to the gadget's last-aware
    /// time here -- that substitution is a rendering decision and is made in
    /// [`DecorationOut`], where a finite lifespan is actually required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<i64>,
    /// The arcdps "wrbg" owner, as a name: `white` (unowned), `red`, `blue`
    /// or `green`. An owner index arcdps adds later serializes as
    /// `unknown_<n>` rather than folding into `white`, so a new value stays
    /// visible instead of reading as neutral.
    pub original_owner: String,
    /// Absent when no geometry row ever arrived, which is GW2EI's `IsValid`
    /// being false. Such an area produces no decoration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<CaptureShapeOut>,
    pub owner_states: Vec<OwnerStateOut>,
    pub progress_states: Vec<ProgressStateOut>,
}

/// The capture area's geometry, in WORLD coordinates (the decoration form
/// carries polygon vertices relative to the anchor instead).
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureShapeOut {
    /// A circle centred on the gadget. Note this is the arcdps single-point
    /// overload, not a degenerate polygon -- see
    /// [`axilog_core::analysis::gadget_capture::CaptureShape`].
    Circle { radius: f32 },
    Polygon { points: Vec<(f32, f32)> },
}

/// One owner transition: at `time_ms` the area was held by `from` and being
/// taken by `by`. Both use the same owner naming as
/// [`CaptureOut::original_owner`].
#[derive(Serialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct OwnerStateOut {
    pub time_ms: i64,
    pub from: String,
    pub by: String,
}

/// A run of progress samples sharing one owner pair.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct ProgressStateOut {
    pub from: String,
    pub by: String,
    /// `by` is nobody (white), so the bar is falling back toward `from`
    /// rather than being captured. Carried rather than left to the reader to
    /// derive from `by == "white"`, because that derivation is exactly the
    /// kind of implicit rule that goes wrong once a new owner index exists.
    pub decaying: bool,
    /// `(time_ms, percent)`, percent in `0.0..=100.0` at 2 decimal places.
    pub progress: Vec<(i64, f64)>,
}

/// One drawable environment decoration.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct DecorationOut {
    /// `capture_outline` or `capture_progress`.
    pub kind: String,
    /// Log-relative ms, SIGNED. Unlike every other time in this format this
    /// can be negative by exactly one millisecond: the capture-progress
    /// splitter synthesizes a sample at `time - 1`, which underflows for a
    /// transition landing on log-relative 0. Clamping it to 0 would silently
    /// reorder that sample after the run it closes.
    pub start_ms: i64,
    pub end_ms: i64,
    /// World-space `(x, y)` the shape is drawn around.
    pub anchor: (f32, f32),
    /// CSS `rgba(...)`. For a progress bar the two colour slots do NOT have
    /// fixed owner roles -- see
    /// [`axilog_core::analysis::decorations::Decoration::color`].
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_color: Option<String>,
    pub shape: DecorationShapeOut,
}

/// A decoration's geometry, relative to [`DecorationOut::anchor`].
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecorationShapeOut {
    Circle { radius: f32, filled: bool },
    /// Vertices RELATIVE to the anchor, unlike [`CaptureShapeOut::Polygon`].
    Polygon { points: Vec<(f32, f32)>, filled: bool },
    ProgressBar { width: u32, height: u32, progress: Vec<(i64, f64)> },
}

/// One squad entity's activity intervals: the cheap half of the replay,
/// carried whether or not positions were requested.
// `Eq` is gone as of the distance scalars below: they are `f64`, and this
// type is only ever compared for equality in tests.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
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
    /// EI's `distToCom` -- mean distance to the commander over this actor's
    /// active polls, in world inches.
    ///
    /// **Two-state convention, and it is load-bearing here.** `None` means
    /// THE PASS NEVER RAN: `--replay` was not passed, no positions were
    /// decoded, and nothing was measured. `-1.0` means THE PASS RAN AND
    /// NOTHING QUALIFIED: positions were decoded and this actor had no poll
    /// that paired with a commander reference. These must never be
    /// collapsed. A consumer that maps absence to `-1` cannot tell "we did
    /// not look" from "we looked and this actor was never within reach of a
    /// commander", and a consumer that maps `-1` to absence loses EI's own
    /// sentinel, which every EI-shaped reader already rejects by value.
    ///
    /// This is the ONE field group on this struct whose presence depends on
    /// the `--replay` gate; every other field here is computed on every
    /// parse. That is deliberate: the field lives on the always-present
    /// per-entity row, rather than beside the position samples in
    /// [`ReplayTrack`], precisely so that `None` stays reachable and the
    /// two-state convention stays real -- inside the gated half it could
    /// only ever be `Some`. The consequence a reader must hold: an
    /// invariance check of the shape "gating positions on must not change
    /// this row" applies to the interval fields (`start_ms`, `end_ms`,
    /// `active_ms`, `down`, `dead`, `dc`) and NOT to these two scalars.
    /// See `axilog_core::analysis::distance` for the five semantics behind
    /// the number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dist_to_com: Option<f64>,
    /// EI's `stackDist` -- the same reduction against the squad centre.
    /// Same two-state convention, same `--replay` gate dependence, as
    /// [`ReplayIntervals::dist_to_com`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_dist: Option<f64>,
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
    /// The static geometry that turns the world coordinates in
    /// [`ReplayTrack::samples`] into a picture. Rides `tracks` for the same
    /// reason `poll_ms` and `bounds` do: it describes the samples and is
    /// meaningless without them.
    ///
    /// `None` when the log's map id has no known arena (see [`ArenaOut`]);
    /// consumers then have only `bounds`, which is the union of the observed
    /// positions rather than a fixed frame, and is therefore NOT comparable
    /// between two logs on the same map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arena: Option<ArenaOut>,
    /// Keyed by entity id. The legacy `ReplayTrackOut` carried no join key
    /// at all, so a consumer could not tell whose track it was reading.
    pub by_entity: ByEntity<ReplayTrack>,
}

/// The fixed world rectangle a WvW map's arena image covers, and the image
/// itself: everything a consumer needs to project [`ReplayTrack::samples`]
/// onto a map without knowing anything about GW2 map geometry.
///
/// # Why this is emitted rather than left to the consumer
///
/// Positions in this format are raw world (game-inch) coordinates, which is
/// the honest thing to carry -- they are what arcdps records and they are
/// projection-independent. But they are unplottable on their own: turning
/// them into map pixels needs the per-map world rect, which is static GW2
/// data axilog already holds in [`axilog_core::wvw::maps::WVW_MAPS`]. Making
/// each consumer re-transcribe that table would recreate exactly the
/// drift that module's doc comment exists to prevent, one repository
/// further out. So the rect travels with the samples.
///
/// # Projection
///
/// World y grows northward, image y grows downward, so the y axis flips:
///
/// ```text
/// px = (x - world_min_x) / (world_max_x - world_min_x) * image_width
/// py = (1 - (y - world_min_y) / (world_max_y - world_min_y)) * image_height
/// ```
///
/// Scale both by `canvas / image_*` to render at any size. Nothing here is
/// pre-rounded or pre-rescaled: GW2EI's exported `combatReplayMetaData`
/// carries the image size already squeezed to a 750px maximum dimension and
/// an `inchToPixel` rounded to three decimals, both of which are artifacts
/// of its own renderer. A consumer that wants those numbers can derive
/// them; a consumer that wants full precision cannot recover it from them.
#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
pub struct ArenaOut {
    /// The arena image's native width in pixels (GW2EI's `_pixelSize.width`).
    pub image_width: u32,
    /// The arena image's native height in pixels.
    pub image_height: u32,
    /// The arena image URL.
    pub image_url: &'static str,
    /// World (game-inch) x of the image's LEFT edge.
    pub world_min_x: f64,
    /// World y of the image's BOTTOM edge -- the larger `py`, because of the
    /// flip documented above.
    pub world_min_y: f64,
    /// World x of the image's RIGHT edge.
    pub world_max_x: f64,
    /// World y of the image's TOP edge.
    pub world_max_y: f64,
}

impl ArenaOut {
    /// Look up the arena for a map id. `None` for every id without a
    /// hand-authored arena image -- see
    /// [`axilog_core::wvw::maps::map_def`], which is the single table this
    /// reads.
    pub fn for_map_id(map_id: u32) -> Option<Self> {
        let def = axilog_core::wvw::maps::map_def(map_id)?;
        let (min_x, min_y, max_x, max_y) = def.rect;
        Some(Self {
            image_width: def.pixel_size.0,
            image_height: def.pixel_size.1,
            image_url: def.image_url,
            world_min_x: min_x,
            world_min_y: min_y,
            world_max_x: max_x,
            world_max_y: max_y,
        })
    }

    /// Project one world position to a pixel in this arena's native image
    /// space, per the formula in the type's doc comment.
    pub fn to_image_pixel(&self, x: f64, y: f64) -> (f64, f64) {
        let fx = (x - self.world_min_x) / (self.world_max_x - self.world_min_x);
        let fy = (y - self.world_min_y) / (self.world_max_y - self.world_min_y);
        (fx * f64::from(self.image_width), (1.0 - fy) * f64::from(self.image_height))
    }
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
    /// Boons the squad removed from enemies, per second. Folded from the
    /// same `support::outgoing_boon_strips` primitive as the `strips`
    /// scalar, so this lane sums to the squad total by construction.
    pub strips: SeriesOut,
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
    /// Cumulative INCOMING healing from the arcdps healing extension --
    /// the receiver-indexed counterpart of `healing_1s`, on the same grid.
    /// Same gate: `timeseries: true` on a log carrying the extension.
    ///
    /// ALLY-ATTRIBUTED, unlike `healing_1s`: a heal only lands here when
    /// its recipient is one of the enumerated players, so this is
    /// "incoming healing from tracked recipients", not "total incoming
    /// healing" -- see `PlayerHealingDetail::healing_received_1s`'s doc
    /// comment for why (no row to put an untracked recipient's heal on).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healing_received_1s: Option<SeriesOut>,
    /// Cumulative INCOMING barrier, same grid and same gate, and the same
    /// ally attribution as [`Self::healing_received_1s`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barrier_received_1s: Option<SeriesOut>,
    /// Outgoing crowd control applied by this entity, per second. Gated on
    /// `timeseries` like the other per-entity lanes; sums to
    /// `blocks.cc.by_entity[id].applied_total`.
    ///
    /// PER-BUCKET, not cumulative -- unlike the three healing lanes above,
    /// which are GW2EI-shaped running totals. The squad-level
    /// `SquadSeries::cc_applied` this decomposes is per-bucket too, so a
    /// consumer summing a row and a column of the player x time grid gets
    /// the same number either way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_applied: Option<SeriesOut>,
    /// Boons this entity removed from enemies, per second. Sums to
    /// `blocks.support.by_entity[id].strips`. Per-bucket, same gate and
    /// same grid as [`Self::cc_applied`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strips: Option<SeriesOut>,
    /// Boons removed FROM this entity, per second. Sums to
    /// `blocks.defenses.by_entity[id].boon_strips_taken`. Per-bucket, same
    /// gate and same grid as [`Self::cc_applied`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strips_taken: Option<SeriesOut>,
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
        healing_received_1s: None,
        barrier_received_1s: None,
        cc_applied: None,
        strips: None,
        strips_taken: None,
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
    entity_series: Option<&axilog_core::analysis::entity_series::EntitySeriesDetail>,
) -> SeriesBlock {
    // Same positional-join guard `support::build_healing` applies to the
    // other half of this pass's output, and for the same reason.
    let healing_1s = healing_1s.filter(|d| d.len() == report.players.len());
    // Same guard, same reason: `EntitySeriesDetail` is a `Vec` over
    // `enc.players` with no addr in it, so a length that disagrees with
    // this report's roster would misattribute every lane rather than fail.
    // Dropping the whole pass is the honest answer -- absent means "not
    // measured", which is exactly what a consumer needs to hear.
    let entity_series = entity_series.filter(|d| d.len() == report.players.len());
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
        strips: SeriesOut::encode_u64(
            res,
            &ps.strips.iter().map(|v| u64::from(*v)).collect::<Vec<_>>(),
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
        // Receiver-indexed counterparts of `healing`, same grid, same gate
        // -- see `EntitySeries::healing_received_1s`'s doc comment.
        let healing_received =
            healing_1s.map(|d| SeriesOut::encode_u64(res, &d[i].healing_received_1s));
        let barrier_received =
            healing_1s.map(|d| SeriesOut::encode_u64(res, &d[i].barrier_received_1s));
        // The CC/strip lanes, on the same positional join and the same
        // `--timeseries` gate. `u32` upstream (they are event COUNTS, not
        // damage), widened here because the envelope has one integer
        // encoder.
        let cc_applied = entity_series.map(|d| {
            SeriesOut::encode_u64(res, &d.get(i).cc_applied.iter().map(|v| u64::from(*v)).collect::<Vec<_>>())
        });
        let strips = entity_series.map(|d| {
            SeriesOut::encode_u64(res, &d.get(i).strips.iter().map(|v| u64::from(*v)).collect::<Vec<_>>())
        });
        let strips_taken = entity_series.map(|d| {
            SeriesOut::encode_u64(res, &d.get(i).strips_taken.iter().map(|v| u64::from(*v)).collect::<Vec<_>>())
        });
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
                        healing_received_1s: healing_received,
                        barrier_received_1s: barrier_received,
                        cc_applied,
                        strips,
                        strips_taken,
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
                healing_received_1s: healing_received,
                barrier_received_1s: barrier_received,
                cc_applied,
                strips,
                strips_taken,
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
    let mut referenced: BTreeSet<i32> = BTreeSet::new();
    for p in &report.players {
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let Some(mods) = p.damage_mods.as_ref() else { continue };
        let mut overall = BTreeMap::new();
        // `id` is signed and already distinguishes outgoing (positive) from
        // incoming (negative) -- see `DamageModEntryOut::id`'s doc comment
        // -- so both directions share one map without collision.
        for m in mods.outgoing.iter().chain(mods.incoming.iter()) {
            cats.reference_damage_mod(m.id);
            referenced.insert(m.id);
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
                referenced.insert(mod_id);
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
    // `personalDamageMods`, carried through from the engine on the legacy
    // report -- see `DamageModsBlock::personal`. Narrowed to `referenced`
    // rather than copied wholesale so it can never name an id the catalog
    // above does not describe.
    let personal = report
        .personal_damage_mods
        .as_ref()
        .map(|m| {
            m.iter()
                .filter_map(|(spec, ids)| {
                    let kept: Vec<i32> =
                        ids.iter().copied().filter(|id| referenced.contains(id)).collect();
                    (!kept.is_empty()).then(|| (spec.clone(), kept))
                })
                .collect()
        })
        .unwrap_or_default();
    DamageModsBlock { by_entity, personal }
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
    extras: Option<&axilog_core::analysis::replay_extras::ReplayExtras>,
) -> ReplayBlock {
    let activity = activity.filter(|a| a.len() == report.players.len());
    // The scalars ride the replay tracks (`#[serde(skip)]` on the legacy
    // shape) and are re-keyed onto entity ids here, so that a player whose
    // track is missing gets `None` -- "the pass did not produce this" --
    // rather than the `-1.0` that means "it did, and nothing qualified".
    let scalars: std::collections::BTreeMap<u64, (f64, f64)> = report
        .replay
        .iter()
        .flat_map(|r| r.tracks.iter())
        .filter_map(|t| {
            index.by_agent_addr(t.agent_addr).map(|id| (u64::from(id), (t.dist_to_com, t.stack_dist)))
        })
        .collect();
    let mut intervals = ByEntity::default();
    if let Some(activity) = activity {
        for (p, a) in report.players.iter().zip(activity) {
            let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
            let scalar = scalars.get(&u64::from(id)).copied();
            intervals.insert(
                id,
                ReplayIntervals {
                    start_ms: a.start_ms,
                    end_ms: a.end_ms,
                    active_ms: a.active_ms(),
                    down: a.down_intervals.iter().map(|i| (i.start_ms, i.end_ms)).collect(),
                    dead: a.dead_intervals.iter().map(|i| (i.start_ms, i.end_ms)).collect(),
                    dc: a.dc_intervals.iter().map(|i| (i.start_ms, i.end_ms)).collect(),
                    dist_to_com: scalar.map(|s| s.0),
                    stack_dist: scalar.map(|s| s.1),
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
            arena: report.encounter.map_id.and_then(ArenaOut::for_map_id),
            by_entity,
        }
    });

    let (gliding, transformations, captures, decorations) =
        extras.map_or_else(Default::default, |e| build_replay_extras(e, index));

    ReplayBlock { by_entity: intervals, tracks, gliding, transformations, captures, decorations }
}

/// Reproject the three eye-candy families onto the wire.
///
/// The only real work here is the entity join, which is best-effort by
/// design (see [`ReplayBlock::gliding`]) -- every row survives whether or not
/// its agent is a tracked entity.
fn build_replay_extras(
    extras: &axilog_core::analysis::replay_extras::ReplayExtras,
    index: &EntityIndex,
) -> (Vec<GliderOut>, Vec<TransformationOut>, Vec<CaptureOut>, Vec<DecorationOut>) {
    use axilog_core::analysis::decorations::{DecorationKind, DecorationShape};
    use axilog_core::analysis::gadget_capture::CaptureShape;

    let gliding = extras
        .agent_states
        .gliding
        .iter()
        .map(|g| GliderOut {
            entity_id: index.by_agent_addr(g.agent_addr),
            agent_addr: g.agent_addr,
            start_ms: g.start_ms,
            end_ms: g.end_ms,
        })
        .collect();

    let transformations = extras
        .agent_states
        .transformations
        .iter()
        .map(|t| TransformationOut {
            entity_id: index.by_agent_addr(t.agent_addr),
            agent_addr: t.agent_addr,
            transformation_id: t.transformation_id,
            guid: t.guid.clone(),
            start_ms: t.start_ms,
            end_ms: t.end_ms,
        })
        .collect();

    let captures = extras
        .captures
        .iter()
        .map(|c| CaptureOut {
            entity_id: index.by_agent_addr(c.agent_addr),
            agent_addr: c.agent_addr,
            start_ms: c.start_ms,
            end_ms: c.end_ms,
            original_owner: owner_name(c.original_owner),
            shape: c.shape.as_ref().map(|s| match s {
                CaptureShape::Circle { radius } => CaptureShapeOut::Circle { radius: *radius },
                CaptureShape::Polygon { points } => {
                    CaptureShapeOut::Polygon { points: points.clone() }
                }
            }),
            owner_states: c
                .owner_states
                .iter()
                .map(|s| OwnerStateOut {
                    time_ms: s.time_ms,
                    from: owner_name(s.from),
                    by: owner_name(s.by),
                })
                .collect(),
            progress_states: c
                .progress_states
                .iter()
                .map(|p| ProgressStateOut {
                    from: owner_name(p.from),
                    by: owner_name(p.by),
                    decaying: p.is_decaying(),
                    progress: p.progress.clone(),
                })
                .collect(),
        })
        .collect();

    let decorations = extras
        .decorations
        .iter()
        .map(|d| DecorationOut {
            kind: match d.kind {
                DecorationKind::CaptureOutline => "capture_outline".to_string(),
                DecorationKind::CaptureProgress => "capture_progress".to_string(),
            },
            start_ms: d.start_ms,
            end_ms: d.end_ms,
            anchor: d.anchor,
            color: d.color.clone(),
            secondary_color: d.secondary_color.clone(),
            shape: match &d.shape {
                DecorationShape::Circle { radius, filled } => {
                    DecorationShapeOut::Circle { radius: *radius, filled: *filled }
                }
                DecorationShape::Polygon { points, filled } => {
                    DecorationShapeOut::Polygon { points: points.clone(), filled: *filled }
                }
                DecorationShape::ProgressBar { width, height, progress } => {
                    DecorationShapeOut::ProgressBar {
                        width: *width,
                        height: *height,
                        progress: progress.clone(),
                    }
                }
            },
        })
        .collect();

    (gliding, transformations, captures, decorations)
}

/// The wrbg owner as a wire name. `Unknown` keeps its raw index rather than
/// folding into `white`, so an owner value arcdps adds later is visible in
/// the output instead of silently reading as unowned.
fn owner_name(owner: axilog_core::analysis::gadget_capture::Owner) -> String {
    use axilog_core::analysis::gadget_capture::Owner;
    match owner {
        Owner::White => "white".to_string(),
        Owner::Red => "red".to_string(),
        Owner::Blue => "blue".to_string(),
        Owner::Green => "green".to_string(),
        Owner::Unknown(n) => format!("unknown_{n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::blocks::tests_support::fixture_report;

    #[test]
    fn squad_series_use_the_shared_envelope_and_decode_to_the_legacy_arrays() {
        let (report, index) = fixture_report();
        let block = build_series(&report, &index, None, None, None, None);
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
        let block = build_replay(&report, &index, None, None);
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
    fn replay_interval_rows_serialize_exactly_the_documented_field_names() {
        // These names ARE the wire contract: the Node and Python suites read
        // them by string and axibridge joins on them. Nothing else pins them.
        // The SDK invariance tests project each row down to the interval
        // fields and compare the projections, which is blind to a rename --
        // both sides lose the field together and stay equal. And `dc` is `[]`
        // on every row of every committed fixture, so no data-bearing
        // assertion reaches it either. This test is the guard.
        //
        // Sorted because serde_json's map is a BTreeMap here (no
        // `preserve_order` feature), so serialization order is alphabetical
        // and not itself part of the contract.
        let keys = |row: &ReplayIntervals| -> Vec<String> {
            let value = serde_json::to_value(row).expect("serializable");
            let mut keys: Vec<String> =
                value.as_object().expect("a row is a JSON object").keys().cloned().collect();
            keys.sort();
            keys
        };
        assert_eq!(
            keys(&ReplayIntervals::default()),
            ["active_ms", "dc", "dead", "down", "end_ms", "start_ms"],
            "the ungated row: the distance scalars are absent when the position pass never ran"
        );
        assert_eq!(
            keys(&ReplayIntervals {
                dist_to_com: Some(-1.0),
                stack_dist: Some(-1.0),
                ..Default::default()
            }),
            [
                "active_ms",
                "dc",
                "dead",
                "dist_to_com",
                "down",
                "end_ms",
                "stack_dist",
                "start_ms"
            ],
            "the gated row: -1.0 must serialize -- it is EI's 'nothing qualified' \
             sentinel, which is a measurement, not absence"
        );
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

/// [`ArenaOut`] is the native format's answer to "where does this position
/// go on a map", and the only existing, log-verified answer to that question
/// in this repository is `ei_replay::MapTransform`, which is a transcription
/// of GW2EI's own renderer. These tests pin the new type to that one.
#[cfg(test)]
mod arena_tests {
    use super::ArenaOut;
    use axilog_core::analysis::ei_replay::MapTransform;
    use axilog_core::wvw::maps::WVW_MAPS;

    #[test]
    fn every_table_map_has_an_arena_and_nothing_else_does() {
        for def in WVW_MAPS {
            let arena = ArenaOut::for_map_id(def.map_id).expect("table entry");
            assert_eq!((arena.image_width, arena.image_height), def.pixel_size);
            assert_eq!(arena.image_url, def.image_url);
            assert_eq!(
                (arena.world_min_x, arena.world_min_y, arena.world_max_x, arena.world_max_y),
                def.rect,
            );
        }
        // GW2EI names these but has no arena image for them; they fall
        // through to a computed bounding box, which is not a fixed frame and
        // must not be presented as one.
        assert!(ArenaOut::for_map_id(899).is_none(), "Obsidian Sanctum");
        assert!(ArenaOut::for_map_id(1315).is_none(), "Armistice Bastion");
        assert!(ArenaOut::for_map_id(0).is_none());
    }

    /// The equality oracle. A consumer that scales this arena's native image
    /// pixels to GW2EI's own canvas must land on GW2EI's own pixel, to the
    /// rounding EI applies at the end -- otherwise the documented projection
    /// is not the projection this data was produced under, and every replay
    /// drawn from it would be subtly displaced.
    #[test]
    fn projection_reproduces_gw2eis_transform_on_every_map() {
        for def in WVW_MAPS {
            let arena = ArenaOut::for_map_id(def.map_id).unwrap();
            let ei = MapTransform::for_map_id(def.map_id).unwrap();
            let sx = ei.out_w / ei.img_w;
            let sy = ei.out_h / ei.img_h;
            // Sample the rect's interior on a coarse grid, plus its corners.
            for i in 0..=8 {
                for j in 0..=8 {
                    let x = (def.rect.0 + (def.rect.2 - def.rect.0) * f64::from(i) / 8.0) as f32;
                    let y = (def.rect.1 + (def.rect.3 - def.rect.1) * f64::from(j) / 8.0) as f32;
                    let (mine_x, mine_y) = arena.to_image_pixel(f64::from(x), f64::from(y));
                    let theirs = ei.to_map_pixel(x, y);
                    // EI rounds to 3 decimals at the very end (`round_ei`);
                    // compare inside that tolerance rather than re-deriving
                    // its rounding, so this test fails on a real projection
                    // error and not on a last-digit tie.
                    assert!(
                        (mine_x * sx - theirs[0]).abs() < 1e-3,
                        "map {} x at ({x}, {y}): {} vs EI {}",
                        def.map_id,
                        mine_x * sx,
                        theirs[0],
                    );
                    assert!(
                        (mine_y * sy - theirs[1]).abs() < 1e-3,
                        "map {} y at ({x}, {y}): {} vs EI {}",
                        def.map_id,
                        mine_y * sy,
                        theirs[1],
                    );
                }
            }
        }
    }

    /// The y flip is the one thing a consumer is most likely to get backwards,
    /// so it gets an assertion that reads like the claim: the world's NORTH
    /// edge is the image's TOP row.
    #[test]
    fn world_north_is_image_top() {
        let arena = ArenaOut::for_map_id(95).unwrap();
        let (_, top) = arena.to_image_pixel(0.0, arena.world_max_y);
        let (_, bottom) = arena.to_image_pixel(0.0, arena.world_min_y);
        assert!(top.abs() < 1e-9, "north edge maps to py 0, got {top}");
        assert!(
            (bottom - f64::from(arena.image_height)).abs() < 1e-9,
            "south edge maps to py = image height, got {bottom}",
        );
        let (left, _) = arena.to_image_pixel(arena.world_min_x, 0.0);
        let (right, _) = arena.to_image_pixel(arena.world_max_x, 0.0);
        assert!(left.abs() < 1e-9, "west edge maps to px 0, got {left}");
        assert!((right - f64::from(arena.image_width)).abs() < 1e-9);
    }
}
