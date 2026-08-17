//! Combat-replay decorations: shapes with a lifespan, a colour and a world
//! anchor.
//!
//! ## Scope, and what this deliberately is not
//!
//! GW2EI's decoration machinery is large -- a metadata/rendering-data split
//! with signature deduplication, a dozen shape types, connectors that follow
//! agents or interpolate between them, skill/buff back-references, and a
//! renderer contract for its HTML report. `analysis::ei_replay`'s
//! `MapTransform` says outright that "the decoration/viewpoint machinery is
//! out of scope", and that was the right call for a map transform.
//!
//! This module opens exactly the door the capture-point work needs and no
//! more: the three shape kinds GW2EI's ONE environment-decoration producer
//! emits, anchored to a fixed world position. Specifically absent, and
//! absent on purpose rather than by oversight:
//!
//! - **Agent-following connectors.** Every decoration here has a fixed
//!   `(x, y)`. GW2EI's `PositionConnector` is what its capture-point
//!   producer uses too, so nothing is lost for this family -- but an
//!   agent-anchored decoration would need a per-frame position lookup, which
//!   is the expensive part of a replay and belongs with whatever renders it.
//! - **Metadata deduplication.** GW2EI hashes shape+colour into a shared
//!   metadata table because its HTML payload repeats decorations thousands
//!   of times. A handful of capture points per WvW log does not, and a
//!   dedupe table would be an unreadable wire format bought with nothing.
//! - **World -> map-pixel projection.** Coordinates here are world inches,
//!   the same space `analysis::replay::Sample` uses. Projection is the
//!   renderer's job and `ei_replay::MapTransform` already owns it.
//!
//! ## Colour
//!
//! Colours are emitted as CSS `rgba(...)` strings, matching GW2EI's own
//! `Color.WithAlpha(opacity).ToString(true)` output, so a consumer can hand
//! them to a canvas without a palette. The four capture-point colours are
//! GW2EI's: white `(255,255,255)`, red `(255,0,0)`, blue `(0,140,255)` --
//! its `LightBlue`, not a pure blue -- and green `(0,255,0)`.

use std::collections::BTreeMap;

use crate::analysis::gadget_capture::{CaptureShape, GadgetCapture, Owner};
use crate::evtc::event::sc;
use crate::evtc::RawLog;

/// What produced a decoration, so a consumer can filter or style by family
/// without pattern-matching on geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    /// The capture area's outline, coloured by its owner.
    CaptureOutline,
    /// The capture progress bar floating at the area's anchor.
    CaptureProgress,
}

/// A decoration's geometry, relative to its [`Decoration::anchor`].
#[derive(Debug, Clone, PartialEq)]
pub enum DecorationShape {
    Circle {
        radius: f32,
        /// GW2EI's `UsingFilled(false)` on every capture outline: outline
        /// only, no fill.
        filled: bool,
    },
    /// Vertices RELATIVE to the anchor, matching GW2EI's
    /// `GetRelativePoints(position)`. Relative rather than absolute because
    /// that is what its renderer consumes, and because it keeps the shape
    /// meaningful if a consumer ever re-anchors it.
    Polygon {
        points: Vec<(f32, f32)>,
        filled: bool,
    },
    /// A progress bar: `width` x `height` in map units, drawn in
    /// [`Decoration::color`] and [`Decoration::secondary_color`] as
    /// `progress` advances.
    ProgressBar {
        width: u32,
        height: u32,
        /// `(time_ms, percent)` samples, linearly interpolated between --
        /// GW2EI's `InterpolationMethod.Linear`, its default.
        progress: Vec<(i64, f64)>,
    },
}

/// One drawable decoration over a closed `[start_ms, end_ms]` lifespan.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoration {
    pub kind: DecorationKind,
    /// Log-relative ms. `i64`, not `u64`, because the capture-point progress
    /// splitter genuinely synthesizes a sample at `time - 1` (see
    /// `gadget_capture::add_progress`), which is negative for a transition
    /// landing on log-relative 0.
    pub start_ms: i64,
    pub end_ms: i64,
    /// World-space `(x, y)` the shape is drawn around.
    pub anchor: (f32, f32),
    /// CSS `rgba(...)`. For an outline this is simply the owner's colour.
    /// For a progress bar it is EI's first colour argument, which is NOT a
    /// fixed role: on a capturing run it is the capper, on a decaying run it
    /// is the holder, because EI swaps its two colour arguments between
    /// those two cases. Reproduced rather than normalised -- normalising
    /// would make this project's bars render inverted against every
    /// EI-shaped consumer.
    pub color: String,
    /// EI's second colour argument for a progress bar, at its higher
    /// opacity. `None` for the shape kinds that have only one colour.
    pub secondary_color: Option<String>,
    pub shape: DecorationShape,
}

/// GW2EI's four wrbg capture colours, at the opacity its producer uses.
fn owner_color(owner: Owner, alpha: f64) -> String {
    let (r, g, b) = match owner {
        // GW2EI's `default:` arm folds everything unrecognised into white,
        // and for a *colour* that is the right call -- there is no fifth
        // palette entry to pick. The decode layer keeps the raw value
        // (`Owner::Unknown`) so the information survives even though the
        // rendering cannot express it.
        Owner::White | Owner::Unknown(_) => (255, 255, 255),
        Owner::Red => (255, 0, 0),
        // GW2EI's `Colors.LightBlue`, not a pure blue.
        Owner::Blue => (0, 140, 255),
        Owner::Green => (0, 255, 0),
    };
    format!("rgba({r},{g},{b},{alpha})")
}

/// GW2EI's decoration opacity for every capture outline and the progress
/// bar's unfilled half.
const OUTLINE_ALPHA: f64 = 0.3;
/// The progress bar's filled half.
const BAR_FILL_ALPHA: f64 = 0.6;
/// GW2EI's fixed progress-bar height.
const BAR_HEIGHT: u32 = 30;

/// Environment decorations for a whole log -- currently the capture-point
/// family only, which is also the only family GW2EI's own
/// `ComputeEnvironmentCombatReplayDecorations` produces for a WvW log.
///
/// (That method also emits icon decorations for GROUND squad markers,
/// `CBTS_SQUADMARKER_GROUND` (53). That is a different statechange from the
/// agent-attached `CBTS_MARKER` (37) this project decodes in `wvw::markers`,
/// carries a world position rather than an agent, and is not decoded here --
/// it is its own backlog item, not part of this one.)
pub fn build_environment_decorations(raw: &RawLog, captures: &[GadgetCapture]) -> Vec<Decoration> {
    // Bail before touching the event stream at all on the overwhelmingly
    // common case: arcdps emits nothing in this family before build
    // 20260602, so every older log has zero captures and must cost zero.
    if captures.is_empty() {
        return Vec::new();
    }
    let tracks = gadget_tracks(raw, captures);

    let mut out = Vec::new();
    for capture in captures {
        // GW2EI skips an invalid capture outright: with no geometry there is
        // nothing to draw, and there is no sensible default radius to invent.
        if !capture.is_valid() {
            continue;
        }
        let Some(track) = tracks.get(&capture.agent_addr) else { continue };
        // GW2EI's `if (src.TryGetCurrentPosition(...))` guard. A capture
        // gadget with no position telemetry cannot be placed on the map.
        let Some(anchor) = track.position_at(capture.start_ms) else { continue };
        // GW2EI substitutes `Src.LastAware` for a capture that never got a
        // HIDE row, and so does this: a decoration must have a finite
        // lifespan, and collapsing it to the capture's own start would make
        // an un-hidden capture point invisible instead.
        let end_ms = capture.end_ms.unwrap_or(track.last_aware_ms);
        push_capture_decorations(&mut out, capture, anchor, end_ms);
    }
    out
}

/// What the decoration layer needs to know about one capture gadget.
struct GadgetTrack {
    /// `(time_ms, x, y)` from `CBTS_POSITION`, in event order (which is
    /// ascending time).
    positions: Vec<(i64, f32, f32)>,
    /// The gadget's last event of ANY kind, log-relative -- the arcdps agent
    /// table's `last_aware`, which is what GW2EI substitutes for a missing
    /// capture end.
    last_aware_ms: i64,
}

impl GadgetTrack {
    /// The gadget's world position at `time_ms`, by NEAREST sample.
    ///
    /// Nearest rather than interpolated, matching GW2EI's own
    /// `TryGetCurrentPosition` -> `BinarySearchParametricPoints`, which
    /// returns the nearest point and not a lerp (its
    /// `TryGetCurrentInterpolatedPosition` is a separate method its
    /// capture-point producer does not call). Capture gadgets do not move,
    /// so the distinction is academic for this family -- it is matched
    /// anyway so a future caller does not inherit a silently different rule.
    fn position_at(&self, time_ms: i64) -> Option<(f32, f32)> {
        self.positions
            .iter()
            .min_by_key(|(t, _, _)| (t - time_ms).abs())
            .map(|(_, x, y)| (*x, *y))
    }
}

/// One pass over the event stream collecting positions and last-aware for
/// every capture gadget at once.
///
/// One pass, not one per capture: this runs on the always-on path, and a
/// per-capture rescan of a 500k-event WvW log would be a real cost to pay
/// for a cosmetic family.
///
/// Not routed through `analysis::replay::Track` either -- that roster covers
/// squad players and enemy-player representatives, and a capture point is a
/// gadget, so it is never in it.
fn gadget_tracks(raw: &RawLog, captures: &[GadgetCapture]) -> BTreeMap<u64, GadgetTrack> {
    let t0 = raw.log_start_ms();
    let mut tracks: BTreeMap<u64, GadgetTrack> = captures
        .iter()
        .map(|c| (c.agent_addr, GadgetTrack { positions: Vec::new(), last_aware_ms: c.start_ms }))
        .collect();

    for e in &raw.events {
        let t = e.time as i64 - t0 as i64;
        // Last-aware counts an agent appearing on EITHER side, matching the
        // arcdps reference's own agent-table recipe (and GW2EI's
        // `AgentItem.LastAware`). A capture gadget is overwhelmingly a
        // `src_agent`, but anything targeting it puts it in `dst_agent`, and
        // scanning one side only would end its decoration early on exactly
        // those logs.
        //
        // Restricted to COMBAT rows (`is_statechange == 0`), and that
        // restriction is not optional: on a statechange row `dst_agent` is
        // usually payload, not an agent. `CBTS_POSITION` packs x/y into it,
        // and this family's own `OUTLINE_POINT` puts a vertex index there.
        // Reading those as agent ids would let an arbitrary float bit
        // pattern collide with a real gadget addr and silently stretch its
        // lifespan.
        if e.is_statechange == 0 && e.dst_agent != e.src_agent {
            if let Some(track) = tracks.get_mut(&e.dst_agent) {
                track.last_aware_ms = track.last_aware_ms.max(t);
            }
        }
        let Some(track) = tracks.get_mut(&e.src_agent) else { continue };
        track.last_aware_ms = track.last_aware_ms.max(t);
        if e.is_statechange == sc::POSITION {
            let dst = e.dst_agent.to_le_bytes();
            track.positions.push((
                t,
                f32::from_le_bytes(dst[0..4].try_into().expect("4 bytes")),
                f32::from_le_bytes(dst[4..8].try_into().expect("4 bytes")),
            ));
        }
    }
    tracks
}

/// The port of `LogLogic.ComputeEnvironmentCombatReplayDecorations`'s
/// capture-point body (`LogLogic.cs:530-618`).
///
/// Three groups come out of one capture area:
///
/// 1. One outline for the window before the first owner transition,
///    coloured by the area's original owner.
/// 2. One outline per owner transition, coloured by whoever the area is
///    being taken FROM, running until the next transition (or the end).
/// 3. One progress bar per progress run.
///
/// A capture area with no progress rows at all takes EI's "should not
/// happen, but as a safety net" path: a single outline over the whole
/// lifespan.
fn push_capture_decorations(
    out: &mut Vec<Decoration>,
    capture: &GadgetCapture,
    anchor: (f32, f32),
    end_ms: i64,
) {
    let shape = capture.shape.as_ref().expect("validity checked by the caller");
    let relative = relative_shape(shape, anchor);

    if capture.progress_states.is_empty() {
        out.push(Decoration {
            kind: DecorationKind::CaptureOutline,
            start_ms: capture.start_ms,
            end_ms,
            anchor,
            color: owner_color(capture.original_owner, OUTLINE_ALPHA),
            secondary_color: None,
            shape: relative,
        });
        return;
    }

    // GW2EI's bar width: the circle's radius (or the polygon's mean vertex
    // distance from the anchor) divided by 1.5, truncated to an integer.
    let bar_width = match &relative {
        DecorationShape::Circle { radius, .. } => (*radius / 1.5) as u32,
        DecorationShape::Polygon { points, .. } if !points.is_empty() => {
            let total: f32 = points.iter().map(|(x, y)| (x * x + y * y).sqrt()).sum();
            (total / (1.5 * points.len() as f32)) as u32
        }
        _ => 0,
    };

    // Group 1: creation to the first owner transition.
    out.push(Decoration {
        kind: DecorationKind::CaptureOutline,
        start_ms: capture.start_ms,
        end_ms: capture.owner_states[0].time_ms,
        anchor,
        color: owner_color(capture.original_owner, OUTLINE_ALPHA),
        secondary_color: None,
        shape: relative.clone(),
    });

    // Group 2: one outline per owner transition.
    for (i, state) in capture.owner_states.iter().enumerate() {
        let next = capture.owner_states.get(i + 1).map_or(end_ms, |s| s.time_ms);
        out.push(Decoration {
            kind: DecorationKind::CaptureOutline,
            start_ms: state.time_ms,
            end_ms: next,
            anchor,
            // `from`, not `by`: the outline shows who currently HOLDS the
            // point while it is being taken, and the bar shows who is
            // taking it.
            color: owner_color(state.from, OUTLINE_ALPHA),
            secondary_color: None,
            shape: relative.clone(),
        });
    }

    // Group 3: one bar per progress run.
    for (i, run) in capture.progress_states.iter().enumerate() {
        let start = run.progress.first().map_or(capture.start_ms, |p| p.0);
        let end = if run.progress.len() == 1 {
            // A single-sample run has no duration of its own; EI collapses
            // it to a point in time rather than stretching it to the next
            // run.
            start
        } else {
            capture
                .progress_states
                .get(i + 1)
                .and_then(|n| n.progress.first().map(|p| p.0))
                .unwrap_or(end_ms)
        };
        // EI swaps its two colour arguments between the decaying and
        // capturing cases, so which owner lands in which slot is not fixed:
        // capturing puts the CAPPER in the primary slot, decaying puts the
        // HOLDER there. The swap is reproduced exactly; see
        // `Decoration::color`.
        let (primary, secondary) = if run.is_decaying() {
            (owner_color(run.from, OUTLINE_ALPHA), owner_color(run.by, BAR_FILL_ALPHA))
        } else {
            (owner_color(run.by, OUTLINE_ALPHA), owner_color(run.from, BAR_FILL_ALPHA))
        };
        out.push(Decoration {
            kind: DecorationKind::CaptureProgress,
            start_ms: start,
            end_ms: end,
            anchor,
            color: primary,
            secondary_color: Some(secondary),
            shape: DecorationShape::ProgressBar {
                width: bar_width,
                height: BAR_HEIGHT,
                progress: run.progress.clone(),
            },
        });
    }
}

/// GW2EI's `GetRelativePoints`: polygon vertices are re-expressed relative
/// to the anchor. A circle is already anchor-relative (its "points" are a
/// radius, not a vertex), so it passes through.
fn relative_shape(shape: &CaptureShape, anchor: (f32, f32)) -> DecorationShape {
    match shape {
        CaptureShape::Circle { radius } => DecorationShape::Circle { radius: *radius, filled: false },
        CaptureShape::Polygon { points } => DecorationShape::Polygon {
            points: points.iter().map(|(x, y)| (x - anchor.0, y - anchor.1)).collect(),
            filled: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::gadget_capture;
    use crate::evtc::{RawEvent, RawHeader};

    fn ev(statechange: u8, time: u64, src: u64) -> RawEvent {
        RawEvent {
            time,
            src_agent: src,
            dst_agent: 0,
            value: 0,
            buff_dmg: 0,
            overstack: 0,
            skillid: 0,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            buff: 0,
            result: 0,
            is_activation: 0,
            is_buffremove: 0,
            is_ninety: 0, is_fifty: 0, is_moving: 0,
            is_statechange: statechange,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    /// `CBTS_POSITION` packs x/y into `dst_agent` as two little-endian
    /// `f32`s -- the same layout `analysis::replay::decode_position` reads.
    fn position(time: u64, src: u64, x: f32, y: f32) -> RawEvent {
        let mut dst = [0u8; 8];
        dst[0..4].copy_from_slice(&x.to_le_bytes());
        dst[4..8].copy_from_slice(&y.to_le_bytes());
        RawEvent { dst_agent: u64::from_le_bytes(dst), ..ev(sc::POSITION, time, src) }
    }

    fn show(time: u64, src: u64, owner: u8) -> RawEvent {
        RawEvent { buff: owner, ..ev(sc::GADGET_CAPTURE_OUTLINE_SHOW, time, src) }
    }

    fn hide(time: u64, src: u64) -> RawEvent {
        ev(sc::GADGET_CAPTURE_OUTLINE_HIDE, time, src)
    }

    fn point(time: u64, src: u64, index: u64, count: u32, x: f32, y: f32) -> RawEvent {
        RawEvent {
            dst_agent: index,
            overstack: count,
            value: x.to_bits() as i32,
            buff_dmg: y.to_bits() as i32,
            ..ev(sc::GADGET_CAPTURE_OUTLINE_POINT, time, src)
        }
    }

    fn percent(time: u64, src: u64, fraction: f32, from: u8, by: u8) -> RawEvent {
        RawEvent {
            value: fraction.to_bits() as i32,
            buff: from,
            result: by,
            ..ev(sc::GADGET_CAPTURE_SPLIT_PERCENT, time, src)
        }
    }

    fn log(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: String::new(), revision: 1, boss_id: 0 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    fn decorate(raw: &RawLog) -> Vec<Decoration> {
        build_environment_decorations(raw, &gadget_capture::build(raw))
    }

    /// The whole pipeline on a plausible capture: a neutral circle taken by
    /// red. Groups 1-3 of `push_capture_decorations` must all appear.
    #[test]
    fn a_neutral_circle_taken_by_red_produces_outlines_and_a_bar() {
        let raw = log(vec![
            position(0, 9, 1_000.0, 2_000.0),
            point(0, 9, 0, 1, 300.0, 0.0),
            show(0, 9, 0),
            percent(100, 9, 0.5f32, 0, 1),
            percent(200, 9, 0.75f32, 0, 1),
            hide(400, 9),
        ]);
        let d = decorate(&raw);

        // Group 1 (creation -> first transition), group 2 (one per
        // transition), group 3 (one per progress run).
        assert_eq!(d.len(), 3);

        assert_eq!(d[0].kind, DecorationKind::CaptureOutline);
        assert_eq!((d[0].start_ms, d[0].end_ms), (0, 100));
        assert_eq!(d[0].color, "rgba(255,255,255,0.3)", "unowned before the first transition");
        assert_eq!(d[0].anchor, (1_000.0, 2_000.0));
        assert_eq!(d[0].shape, DecorationShape::Circle { radius: 300.0, filled: false });

        assert_eq!(d[1].kind, DecorationKind::CaptureOutline);
        assert_eq!((d[1].start_ms, d[1].end_ms), (100, 400));
        assert_eq!(d[1].color, "rgba(255,255,255,0.3)", "still held by nobody while red caps");

        assert_eq!(d[2].kind, DecorationKind::CaptureProgress);
        assert_eq!((d[2].start_ms, d[2].end_ms), (100, 400));
        assert_eq!(
            d[2].shape,
            DecorationShape::ProgressBar {
                width: 200, // radius 300 / 1.5
                height: BAR_HEIGHT,
                progress: vec![(100, 50.0), (200, 75.0)],
            }
        );
        // Capturing: the CAPPER (red) is EI's first colour argument, the
        // holder (white) its second.
        assert_eq!(d[2].color, "rgba(255,0,0,0.3)");
        assert_eq!(d[2].secondary_color.as_deref(), Some("rgba(255,255,255,0.6)"));
    }

    /// GW2EI's `LightBlue` is the wrbg blue -- a pure `rgba(0,0,255,..)`
    /// here would be a silent palette divergence from every EI-shaped
    /// renderer.
    #[test]
    fn the_wrbg_palette_matches_gw2ei_including_its_light_blue() {
        assert_eq!(owner_color(Owner::White, 0.3), "rgba(255,255,255,0.3)");
        assert_eq!(owner_color(Owner::Red, 0.3), "rgba(255,0,0,0.3)");
        assert_eq!(owner_color(Owner::Blue, 0.3), "rgba(0,140,255,0.3)");
        assert_eq!(owner_color(Owner::Green, 0.3), "rgba(0,255,0,0.3)");
        assert_eq!(
            owner_color(Owner::Unknown(7), 0.3),
            "rgba(255,255,255,0.3)",
            "no fifth palette entry exists; the raw value survives on the decode side instead"
        );
    }

    /// Polygon vertices come out RELATIVE to the anchor, and the bar width
    /// is their mean distance from it over 1.5.
    #[test]
    fn polygon_vertices_are_re_expressed_relative_to_the_anchor() {
        let raw = log(vec![
            position(0, 9, 100.0, 100.0),
            point(0, 9, 0, 4, 100.0, 130.0),
            point(0, 9, 1, 4, 130.0, 100.0),
            point(0, 9, 2, 4, 100.0, 70.0),
            point(0, 9, 3, 4, 70.0, 100.0),
            show(0, 9, 2),
            percent(50, 9, 0.5f32, 2, 3),
            hide(100, 9),
        ]);
        let d = decorate(&raw);
        assert_eq!(
            d[0].shape,
            DecorationShape::Polygon {
                points: vec![(0.0, 30.0), (30.0, 0.0), (0.0, -30.0), (-30.0, 0.0)],
                filled: false,
            }
        );
        assert_eq!(d[0].color, "rgba(0,140,255,0.3)", "blue held it before the first transition");
        // Every vertex is 30 from the anchor, so the mean is 30: 30/1.5 = 20.
        let DecorationShape::ProgressBar { width, .. } = d[2].shape else {
            panic!("expected a progress bar, got {:?}", d[2].shape);
        };
        assert_eq!(width, 20);
    }

    /// A capture with geometry but no progress rows takes EI's explicit
    /// "should not happen, but as a safety net" path: one outline over the
    /// whole lifespan, no bar.
    #[test]
    fn a_capture_with_no_progress_gets_one_outline_over_its_whole_life() {
        let raw = log(vec![
            position(0, 9, 0.0, 0.0),
            point(0, 9, 0, 1, 200.0, 0.0),
            show(10, 9, 3),
            hide(900, 9),
        ]);
        let d = decorate(&raw);
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].start_ms, d[0].end_ms), (10, 900));
        assert_eq!(d[0].color, "rgba(0,255,0,0.3)");
    }

    /// EI's `IsValid` guard: no geometry, no decoration.
    #[test]
    fn a_capture_with_no_geometry_produces_nothing() {
        let raw = log(vec![position(0, 9, 0.0, 0.0), show(0, 9, 1), hide(50, 9)]);
        assert!(decorate(&raw).is_empty());
    }

    /// EI's `TryGetCurrentPosition` guard: a gadget with no position
    /// telemetry cannot be placed, so it is skipped rather than drawn at the
    /// origin.
    #[test]
    fn a_capture_whose_gadget_has_no_position_is_skipped() {
        let raw = log(vec![point(0, 9, 0, 1, 200.0, 0.0), show(0, 9, 1), hide(50, 9)]);
        assert!(decorate(&raw).is_empty());
    }

    /// A capture that never gets a HIDE still needs a finite lifespan; EI
    /// substitutes the gadget's last-aware time and so does this.
    #[test]
    fn an_unhidden_capture_ends_at_the_gadgets_last_aware_time() {
        let raw = log(vec![
            position(0, 9, 0.0, 0.0),
            point(0, 9, 0, 1, 100.0, 0.0),
            show(0, 9, 1),
            position(700, 9, 0.0, 0.0),
        ]);
        let d = decorate(&raw);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].end_ms, 700);
    }

    /// While decaying the two bar colours swap roles. Getting this backwards
    /// renders a decaying point as if it were being capped.
    #[test]
    fn a_decaying_run_swaps_the_bars_two_colours() {
        let raw = log(vec![
            position(0, 9, 0.0, 0.0),
            point(0, 9, 0, 1, 150.0, 0.0),
            show(0, 9, 1),
            // Red holds it, nobody is capping: decaying.
            percent(10, 9, 0.9f32, 1, 0),
            percent(20, 9, 0.8f32, 1, 0),
            hide(30, 9),
        ]);
        let d = decorate(&raw);
        let bar = d.iter().find(|x| x.kind == DecorationKind::CaptureProgress).expect("a bar");
        // Decaying inverts the roles: the HOLDER (red) is now EI's first
        // colour argument and nobody-white its second -- the exact opposite
        // of `a_neutral_circle_taken_by_red_produces_outlines_and_a_bar`.
        assert_eq!(bar.color, "rgba(255,0,0,0.3)");
        assert_eq!(bar.secondary_color.as_deref(), Some("rgba(255,255,255,0.6)"));
    }

    /// A single-sample run collapses to a point in time rather than
    /// stretching to the next run.
    #[test]
    fn a_single_sample_run_has_zero_duration() {
        let raw = log(vec![
            position(0, 9, 0.0, 0.0),
            point(0, 9, 0, 1, 150.0, 0.0),
            show(0, 9, 0),
            percent(10, 9, 0.5f32, 0, 1),
            // Owner change to a mid-range value: no backfill, so run 0 keeps
            // its single sample.
            percent(20, 9, 0.6f32, 0, 2),
            hide(30, 9),
        ]);
        let d = decorate(&raw);
        let bars: Vec<_> = d.iter().filter(|x| x.kind == DecorationKind::CaptureProgress).collect();
        assert_eq!(bars.len(), 2);
        assert_eq!((bars[0].start_ms, bars[0].end_ms), (10, 10));
        assert_eq!((bars[1].start_ms, bars[1].end_ms), (20, 20));
    }
}
