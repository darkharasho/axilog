//! Capture-point areas, assembled from the four `CBTS_GADGETCAPTURE*`
//! statechanges (80-83).
//!
//! This is a port of GW2EI's `GadgetCaptureEvent` plus the assembly half of
//! its `CombatEventFactory` dispatch (`CombatEventFactory.cs:601-637`). It is
//! the only one of the three replay eye-candy families that GW2EI actually
//! renders: `LogLogic.ComputeEnvironmentCombatReplayDecorations`
//! (`LogLogic.cs:530`) turns these into environment replay decorations. That
//! consumer is ported separately, in [`crate::analysis::decorations`], so
//! that this module stays a decode of what the log says and the decoration
//! layer stays a rendering decision.
//!
//! **Four statechanges, one object.** `OUTLINE_SHOW` (80) creates a capture
//! area, `OUTLINE_POINT` (83) supplies its geometry, `SPLIT_PERCENT` (81)
//! supplies capture progress, `OUTLINE_HIDE` (82) ends it. Three of the four
//! address "the current capture area for this `src_agent`", so the whole
//! family is a per-agent state machine rather than four independent decodes.
//!
//! **The point/show ordering rule is load-bearing and counterintuitive.**
//! Geometry rows are buffered per agent and flushed into a capture area when
//! its SHOW row arrives, after which the buffer is CLEARED. A point row
//! arriving after the SHOW therefore does not belong to it -- it is held for
//! the next one. This is GW2EI's behaviour exactly
//! (`GadgetCapturePointCombatItemsBySrc` is populated by case 83 and drained
//! and `Remove`d by case 80) and it is reproduced rather than "fixed",
//! because the alternative reading -- attach points to the open capture --
//! would silently reshape every capture area on any log where arcdps emits
//! the rows in the other order.
//!
//! **Build gate.** arcdps emits this family only from build `20260602`
//! onward. The committed WvW fixture is build `20260114`, so it carries zero
//! rows of all four and cannot exercise any of this; coverage here is
//! hand-built wire-shape tests, the same gap class already documented for
//! `encounter.tick_rate` and the sc=74/75 WvW fields.

use std::collections::BTreeMap;

use crate::evtc::event::{sc, RawEvent};
use crate::evtc::RawLog;

/// The arcdps "wrbg" owner index carried by `buff` (capturing *from*) and
/// `result` (capturing *by*) on this family's rows.
///
/// The reference spells it "wrbg colour", and GW2EI's `GadgetCaptureEvent.
/// GetColor` resolves the same four values to White/Red/LightBlue/Green.
/// White is the unowned/neutral state, which is why [`Owner::is_nobody`]
/// exists: `by == White` is GW2EI's `IsDecaying`, i.e. nobody is capping and
/// the bar is falling back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owner {
    /// 0 -- unowned/neutral.
    White,
    Red,
    Blue,
    Green,
    /// Anything else. Kept rather than folded into `White` so a future
    /// arcdps addition is visible instead of silently rendering as neutral;
    /// GW2EI's `default:` arm folds it, which is a rendering choice, not a
    /// decode one.
    Unknown(u8),
}

impl Owner {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Owner::White,
            1 => Owner::Red,
            2 => Owner::Blue,
            3 => Owner::Green,
            other => Owner::Unknown(other),
        }
    }

    /// GW2EI's `IsDecaying` predicate, in its `by` position: nobody is
    /// capturing.
    pub fn is_nobody(self) -> bool {
        self == Owner::White
    }
}

/// The capture area's shape, as supplied by `OUTLINE_POINT` (83).
///
/// The single-point case is not a degenerate polygon -- arcdps overloads it:
/// "if count is 1, shape is a circle around agent with radius x". GW2EI
/// encodes the same overload as `IsCircle => _points.Length == 1` /
/// `Radius => _points[0].X`.
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureShape {
    Circle { radius: f32 },
    /// World-space (x, y) vertices, in `dst_agent` index order. Slots never
    /// filled by a point row stay at the origin, matching GW2EI's
    /// zero-initialised `Vector3[]`.
    Polygon { points: Vec<(f32, f32)> },
}

/// One owner transition on a capture area: at `time_ms` the area went from
/// being owned by `from` to being captured by `by`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OwnerState {
    pub time_ms: i64,
    pub from: Owner,
    pub by: Owner,
}

impl OwnerState {
    /// GW2EI's `IsDecaying`: nobody is capping, so the bar decays back
    /// toward `from`.
    pub fn is_decaying(&self) -> bool {
        self.by.is_nobody()
    }
}

/// A run of progress samples sharing one `(from, by)` owner pair. A new run
/// starts whenever that pair changes.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressState {
    pub from: Owner,
    pub by: Owner,
    /// `(time_ms, percent)` in ascending time order. `percent` is
    /// `0.0..=100.0`, rounded to 2 decimal places exactly as GW2EI rounds it
    /// (`Math.Round(value * 100.0f, 2)`) -- the wire value is a `f32` in
    /// 0.0-1.0 despite the statechange being named `SPLITPERCENT`.
    pub progress: Vec<(i64, f64)>,
}

impl ProgressState {
    pub fn is_decaying(&self) -> bool {
        self.by.is_nobody()
    }

    /// GW2EI's `ProgressState.AddState`, ported including both of its
    /// non-obvious filters.
    ///
    /// A sample that restates the run's own terminal value is dropped: 100%
    /// while decaying, or 0% while capturing, is the value the run STARTED
    /// from, not progress. And if the run's single existing sample is that
    /// same terminal value, the incoming sample REPLACES it instead of being
    /// appended -- otherwise every run would open with a flat leading
    /// segment at its start value.
    fn add(&mut self, sample: (i64, f64)) {
        let terminal = if self.is_decaying() { 100.0 } else { 0.0 };
        if sample.1 == terminal {
            return;
        }
        let replace_first = self.progress.len() == 1 && self.progress[0].1 == terminal;
        if replace_first {
            self.progress[0] = sample;
        } else {
            self.progress.push(sample);
        }
    }
}

/// One capture area over its lifetime.
#[derive(Debug, Clone, PartialEq)]
pub struct GadgetCapture {
    /// The capture gadget's raw agent addr. This is the anchor the
    /// decoration layer resolves a world position for; it is a gadget, so it
    /// is NOT in the replay track roster (which covers squad players and
    /// enemy-player representatives only).
    pub agent_addr: u64,
    /// Log-relative ms of the `OUTLINE_SHOW` row.
    pub start_ms: i64,
    /// Log-relative ms of the `OUTLINE_HIDE` row, or of the next
    /// `OUTLINE_SHOW` for the same agent. `None` when neither ever arrived.
    ///
    /// GW2EI substitutes `Src.LastAware` here. This does not, because
    /// last-aware is a property of the agent table that this pass has no
    /// business asserting -- the decoration layer, which does need a concrete
    /// end, resolves `None` itself and documents what it resolves it to.
    pub end_ms: Option<i64>,
    /// The owner at creation (`buff` on the SHOW row), later overwritten by
    /// the first progress row's own `from` -- GW2EI does the same
    /// reassignment in `AddProgress`, and it matters because the SHOW row's
    /// value is stale on an area that was already contested when recording
    /// began.
    pub original_owner: Owner,
    /// `None` when no geometry row was ever attached. GW2EI's `IsValid`
    /// (`_points.Length > 0`) is exactly this being `Some`, and its one
    /// consumer skips invalid captures outright.
    pub shape: Option<CaptureShape>,
    pub owner_states: Vec<OwnerState>,
    pub progress_states: Vec<ProgressState>,
}

impl GadgetCapture {
    /// GW2EI's `IsValid`: an area with no geometry cannot be drawn and is
    /// not a capture area in any useful sense.
    pub fn is_valid(&self) -> bool {
        self.shape.is_some()
    }
}

/// Assemble every capture area in the log, in `OUTLINE_SHOW` order.
pub fn build(raw: &RawLog) -> Vec<GadgetCapture> {
    let t0 = raw.log_start_ms();
    let mut out: Vec<GadgetCapture> = Vec::new();
    // The agent's most recent capture area. Every one of the other three
    // statechanges addresses this and only this -- EI reads `[^1]` off its
    // per-Src list in all three cases.
    let mut current: BTreeMap<u64, usize> = BTreeMap::new();
    // Geometry rows waiting for a SHOW. See the module doc for why this is a
    // pre-buffer rather than a post-attach.
    let mut pending_points: BTreeMap<u64, Vec<&RawEvent>> = BTreeMap::new();

    for e in &raw.events {
        let t = e.time as i64 - t0 as i64;
        match e.is_statechange {
            sc::GADGET_CAPTURE_OUTLINE_SHOW => {
                // A second SHOW ends the previous area, exactly as a HIDE
                // would.
                if let Some(&i) = current.get(&e.src_agent) {
                    set_end(&mut out[i], t);
                }
                let mut capture = GadgetCapture {
                    agent_addr: e.src_agent,
                    start_ms: t,
                    end_ms: None,
                    original_owner: Owner::from_u8(e.buff),
                    shape: None,
                    owner_states: Vec::new(),
                    progress_states: Vec::new(),
                };
                if let Some(points) = pending_points.remove(&e.src_agent) {
                    for p in points {
                        add_point(&mut capture, p);
                    }
                }
                current.insert(e.src_agent, out.len());
                out.push(capture);
            }
            sc::GADGET_CAPTURE_OUTLINE_POINT => {
                pending_points.entry(e.src_agent).or_default().push(e);
            }
            sc::GADGET_CAPTURE_OUTLINE_HIDE => {
                if let Some(&i) = current.get(&e.src_agent) {
                    set_end(&mut out[i], t);
                }
            }
            sc::GADGET_CAPTURE_SPLIT_PERCENT => {
                if let Some(&i) = current.get(&e.src_agent) {
                    add_progress(&mut out[i], e, t);
                }
            }
            _ => {}
        }
    }
    out
}

/// GW2EI's `SetEnd`: write-once.
fn set_end(capture: &mut GadgetCapture, time_ms: i64) {
    if capture.end_ms.is_none() {
        capture.end_ms = Some(time_ms);
    }
}

/// GW2EI's `AddPoint`.
///
/// The array is sized from the FIRST point row's `overstack` and never
/// resized; a row whose index falls outside it is dropped. That is EI's
/// behaviour verbatim, and it is the right one: `overstack` is documented as
/// "point count for this agent", so a later row disagreeing with the first is
/// corrupt data, and growing the array to fit it would silently change the
/// shape's vertex count -- which is also what distinguishes a circle from a
/// polygon.
fn add_point(capture: &mut GadgetCapture, e: &RawEvent) {
    let x = f32::from_bits(e.value as u32);
    let y = f32::from_bits(e.buff_dmg as u32);
    let shape = capture.shape.get_or_insert_with(|| {
        if e.overstack == 1 {
            CaptureShape::Circle { radius: 0.0 }
        } else {
            CaptureShape::Polygon { points: vec![(0.0, 0.0); e.overstack as usize] }
        }
    });
    match shape {
        // The circle overload: one "point" whose x IS the radius.
        CaptureShape::Circle { radius } => {
            if e.dst_agent == 0 {
                *radius = x;
            }
        }
        CaptureShape::Polygon { points } => {
            let index = e.dst_agent as usize;
            if index < points.len() {
                points[index] = (x, y);
            }
        }
    }
}

/// GW2EI's `AddProgress`, ported including the owner-change split.
///
/// Three behaviours here are not obvious from the payload and all three are
/// EI's:
///
/// 1. Progress rows are IGNORED once the area has ended. EI guards on
///    `_endIsSet`, so a late row cannot resurrect a hidden capture point.
/// 2. The FIRST progress row overwrites `original_owner`. The SHOW row's
///    `buff` is stale whenever recording began on an already-contested area.
/// 3. On an owner change that lands exactly on 0% or 100%, a synthetic
///    sample is appended to the OUTGOING run at `time - 1` before the new run
///    opens at `time`. Without it the outgoing run's bar jumps rather than
///    completing, because the terminal-value filter in
///    [`ProgressState::add`] would otherwise have dropped that very sample.
fn add_progress(capture: &mut GadgetCapture, e: &RawEvent, time_ms: i64) {
    if capture.end_ms.is_some() {
        return;
    }
    let progress = round2(f32::from_bits(e.value as u32) as f64 * 100.0);
    let from = Owner::from_u8(e.buff);
    let by = Owner::from_u8(e.result);

    if capture.progress_states.is_empty() {
        capture.original_owner = from;
        capture.progress_states.push(ProgressState { from, by, progress: vec![(time_ms, progress)] });
        capture.owner_states.push(OwnerState { time_ms, from, by });
        return;
    }

    let (last_from, last_by, last_progress) = {
        let last = capture.progress_states.last().expect("non-empty, just checked");
        (last.from, last.by, last.progress.last().map(|p| p.1))
    };
    let owner_changed = last_from != from || last_by != by;
    if owner_changed {
        capture.owner_states.push(OwnerState { time_ms, from, by });
        if progress == 100.0 || progress == 0.0 {
            capture.progress_states.last_mut().expect("non-empty").add((time_ms - 1, progress));
        }
        capture
            .progress_states
            .push(ProgressState { from, by, progress: vec![(time_ms, progress)] });
    } else if last_progress != Some(progress) {
        capture.progress_states.last_mut().expect("non-empty").add((time_ms, progress));
    }
}

/// GW2EI's `Math.Round(x, 2)` -- banker's rounding in .NET, but the inputs
/// here are `f32`-derived percentages where a tie at the third decimal is
/// not representable in practice, so half-away-from-zero matches on every
/// reachable value and is what `f64::round` gives.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader};

    fn ev(statechange: u8) -> RawEvent {
        RawEvent {
            time: 0,
            src_agent: 1,
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

    fn show(time: u64, owner: u8) -> RawEvent {
        RawEvent { time, buff: owner, ..ev(sc::GADGET_CAPTURE_OUTLINE_SHOW) }
    }

    fn hide(time: u64) -> RawEvent {
        RawEvent { time, ..ev(sc::GADGET_CAPTURE_OUTLINE_HIDE) }
    }

    /// `count == 1` is the circle overload: `x` is a radius, not a vertex.
    fn point(time: u64, index: u64, count: u32, x: f32, y: f32) -> RawEvent {
        RawEvent {
            time,
            dst_agent: index,
            overstack: count,
            value: x.to_bits() as i32,
            buff_dmg: y.to_bits() as i32,
            ..ev(sc::GADGET_CAPTURE_OUTLINE_POINT)
        }
    }

    /// `fraction` is 0.0-1.0 on the wire despite the "percent" in the name.
    fn percent(time: u64, fraction: f32, from: u8, by: u8) -> RawEvent {
        RawEvent {
            time,
            value: fraction.to_bits() as i32,
            buff: from,
            result: by,
            ..ev(sc::GADGET_CAPTURE_SPLIT_PERCENT)
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

    /// The overload that is easiest to get wrong: a single point row is a
    /// CIRCLE with radius `x`, not a one-vertex polygon.
    #[test]
    fn a_single_point_row_is_a_circle_whose_radius_is_x() {
        let raw = log(vec![point(0, 0, 1, 450.0, 999.0), show(0, 1)]);
        let c = build(&raw);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].shape, Some(CaptureShape::Circle { radius: 450.0 }));
        assert!(c[0].is_valid());
    }

    #[test]
    fn multiple_point_rows_form_a_polygon_in_index_order() {
        let raw = log(vec![
            // Deliberately out of index order on the wire: `dst_agent` is
            // the index, arrival order is not.
            point(0, 2, 3, 30.0, 31.0),
            point(0, 0, 3, 10.0, 11.0),
            point(0, 1, 3, 20.0, 21.0),
            show(0, 0),
        ]);
        let c = build(&raw);
        assert_eq!(
            c[0].shape,
            Some(CaptureShape::Polygon {
                points: vec![(10.0, 11.0), (20.0, 21.0), (30.0, 31.0)]
            })
        );
    }

    /// GW2EI drops an out-of-range index rather than growing the array,
    /// because `overstack` on the first row is the authoritative count.
    #[test]
    fn an_out_of_range_point_index_is_dropped_not_grown_into() {
        let raw = log(vec![point(0, 0, 2, 1.0, 2.0), point(0, 7, 2, 9.0, 9.0), show(0, 0)]);
        let c = build(&raw);
        assert_eq!(
            c[0].shape,
            Some(CaptureShape::Polygon { points: vec![(1.0, 2.0), (0.0, 0.0)] })
        );
    }

    /// The ordering rule from the module doc: points are buffered for the
    /// NEXT show and the buffer is cleared by it. A point arriving after the
    /// show belongs to the show after that.
    #[test]
    fn point_rows_attach_to_the_next_show_not_the_open_capture() {
        let raw = log(vec![
            show(0, 0),
            point(10, 0, 1, 100.0, 0.0),
            hide(20),
            show(30, 0),
        ]);
        let c = build(&raw);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].shape, None, "the first capture never received geometry");
        assert!(!c[0].is_valid());
        assert_eq!(
            c[1].shape,
            Some(CaptureShape::Circle { radius: 100.0 }),
            "the buffered point flushes into the second show"
        );
    }

    #[test]
    fn a_second_show_ends_the_previous_capture_like_a_hide_would() {
        let raw = log(vec![show(0, 1), show(500, 2)]);
        let c = build(&raw);
        assert_eq!(c[0].end_ms, Some(500));
        assert_eq!(c[1].end_ms, None);
    }

    #[test]
    fn set_end_is_write_once() {
        let raw = log(vec![show(0, 1), hide(100), hide(200)]);
        assert_eq!(build(&raw)[0].end_ms, Some(100));
    }

    /// The wire carries a `f32` in 0.0-1.0 bit-punned into the `i32` value
    /// field; EI scales by 100 and rounds to 2dp.
    #[test]
    fn progress_is_a_bit_punned_fraction_scaled_to_a_two_dp_percent() {
        let raw = log(vec![show(0, 0), percent(10, 0.5f32, 0, 1), percent(20, 0.333f32, 0, 1)]);
        let c = build(&raw);
        assert_eq!(c[0].progress_states.len(), 1);
        let p = &c[0].progress_states[0].progress;
        assert_eq!(p[0], (10, 50.0));
        assert_eq!(p[1].1, 33.3, "0.333f32 * 100 rounded to 2dp");
    }

    /// The first progress row overwrites the SHOW row's owner, which is
    /// stale on an area that was already contested when recording began.
    #[test]
    fn the_first_progress_row_overwrites_the_shows_original_owner() {
        let raw = log(vec![show(0, 1), percent(10, 0.4f32, 3, 2)]);
        let c = build(&raw);
        assert_eq!(c[0].original_owner, Owner::Green, "from the progress row, not the show's Red");
    }

    /// A run that is decaying starts at 100 and a run that is capturing
    /// starts at 0; a sample restating that value is not progress. The
    /// single-sample case REPLACES rather than appends, so a run never opens
    /// with a flat leading segment.
    #[test]
    fn a_samples_own_terminal_value_is_filtered_and_replaces_a_lone_first() {
        // Capturing (by = Red, not nobody): terminal is 0.
        let raw = log(vec![
            show(0, 0),
            percent(10, 0.0f32, 0, 1),  // opens the run at 0
            percent(20, 0.0f32, 0, 1),  // same value -- no sample at all
            percent(30, 0.25f32, 0, 1), // replaces the lone 0 rather than appending
            percent(40, 0.5f32, 0, 1),
        ]);
        let c = build(&raw);
        let p = &c[0].progress_states[0].progress;
        assert_eq!(p, &vec![(30, 25.0), (40, 50.0)], "the leading 0 is replaced, not kept");
    }

    /// Decaying is the mirror image: terminal is 100.
    #[test]
    fn a_decaying_runs_terminal_value_is_one_hundred() {
        let raw = log(vec![
            show(0, 0),
            percent(10, 1.0f32, 1, 0), // by = White = nobody = decaying
            percent(20, 0.75f32, 1, 0),
        ]);
        let c = build(&raw);
        assert!(c[0].progress_states[0].is_decaying());
        assert_eq!(c[0].progress_states[0].progress, vec![(20, 75.0)]);
    }

    /// An owner change opens a new run AND records an owner state. When the
    /// change lands exactly on a terminal value, the outgoing run gets a
    /// synthetic sample one ms earlier so its bar completes instead of
    /// jumping -- the terminal-value filter would otherwise have eaten it.
    #[test]
    fn an_owner_change_at_a_terminal_value_backfills_the_outgoing_run() {
        let raw = log(vec![
            show(0, 0),
            percent(10, 0.5f32, 0, 1),
            percent(20, 0.9f32, 0, 1),
            // Red finished capping: 100%, and now Red owns it with nobody
            // capping.
            percent(30, 1.0f32, 1, 0),
        ]);
        let c = build(&raw);
        assert_eq!(c[0].progress_states.len(), 2);
        assert_eq!(
            c[0].progress_states[0].progress.last(),
            Some(&(29, 100.0)),
            "the outgoing run completes at time - 1"
        );
        assert_eq!(c[0].progress_states[1].progress, vec![(30, 100.0)]);
        assert_eq!(
            c[0].owner_states,
            vec![
                OwnerState { time_ms: 10, from: Owner::White, by: Owner::Red },
                OwnerState { time_ms: 30, from: Owner::Red, by: Owner::White },
            ]
        );
    }

    /// A mid-range owner change gets no synthetic backfill -- only the 0/100
    /// case does.
    #[test]
    fn a_mid_range_owner_change_gets_no_backfill() {
        let raw = log(vec![show(0, 0), percent(10, 0.5f32, 0, 1), percent(20, 0.6f32, 0, 2)]);
        let c = build(&raw);
        assert_eq!(c[0].progress_states[0].progress, vec![(10, 50.0)]);
        assert_eq!(c[0].progress_states[1].progress, vec![(20, 60.0)]);
    }

    /// A progress row after the end must not resurrect a hidden area.
    #[test]
    fn progress_after_the_end_is_ignored() {
        let raw = log(vec![show(0, 0), hide(10), percent(20, 0.5f32, 0, 1)]);
        let c = build(&raw);
        assert!(c[0].progress_states.is_empty());
        assert!(c[0].owner_states.is_empty());
    }

    /// Two capture points on one map are two agents; neither may steal the
    /// other's rows.
    #[test]
    fn captures_are_per_agent() {
        let mut p_b = point(0, 0, 1, 200.0, 0.0);
        p_b.src_agent = 2;
        let mut show_b = show(0, 0);
        show_b.src_agent = 2;
        let mut hide_b = hide(50);
        hide_b.src_agent = 2;

        let raw = log(vec![point(0, 0, 1, 100.0, 0.0), show(0, 0), p_b, show_b, hide_b, hide(90)]);
        let c = build(&raw);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].agent_addr, 1);
        assert_eq!(c[0].shape, Some(CaptureShape::Circle { radius: 100.0 }));
        assert_eq!(c[0].end_ms, Some(90));
        assert_eq!(c[1].agent_addr, 2);
        assert_eq!(c[1].shape, Some(CaptureShape::Circle { radius: 200.0 }));
        assert_eq!(c[1].end_ms, Some(50));
    }

    /// Times are log-relative, anchored on the first event.
    #[test]
    fn times_are_log_relative() {
        let raw = log(vec![
            RawEvent { time: 1_000, ..ev(sc::MAP_ID) },
            show(1_500, 0),
            hide(2_500),
        ]);
        let c = build(&raw);
        assert_eq!((c[0].start_ms, c[0].end_ms), (500, Some(1_500)));
    }

    /// An unknown wrbg value must stay visible rather than folding into the
    /// neutral state.
    #[test]
    fn an_unknown_owner_index_is_preserved() {
        assert_eq!(Owner::from_u8(9), Owner::Unknown(9));
        assert!(!Owner::from_u8(9).is_nobody());
        assert!(Owner::from_u8(0).is_nobody());
    }
}
