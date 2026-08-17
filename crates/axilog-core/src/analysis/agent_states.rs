//! Glider and transformation state intervals (`CBTS_GLIDER` = 55,
//! `CBTS_TRANSFORMATION` = 73).
//!
//! **Why this exists even though GW2EI never renders it.** Both families are
//! parsed by GW2EI and then read by nothing: `GliderEvent` has exactly one
//! fetcher (`CombatDataFetchers.cs:821 GetGliderEvents`) and
//! `TransformationEvent` has four (`CombatDataFetchers.cs:292-330`), and no
//! call site in that codebase consumes any of them. So there is no reference
//! output to diff against and no parity to claim -- this is original data.
//! It is decoded here because the arcdps-dev guidance singled these two out
//! (`docs/arcdps-dev-notes.md` row 6) as the source for mount and glider
//! state, which is real information a replay consumer can use and which is
//! otherwise unrecoverable from a parsed log.
//!
//! Because there is no consumer to match, the ONLY thing this module owes
//! GW2EI is its interval-closing rules, which are ported exactly (see
//! [`transformations`]). It deliberately does not invent semantics EI does
//! not have -- notably, `CBTS_TRANSFORMATION`'s documented `value`
//! (duration) field is ignored here exactly as EI ignores it, because the
//! observed close is the `skillid == 0` row and trusting a duration instead
//! would silently disagree with it.
//!
//! Neither family reaches the ei-json output. EI emits nothing for them, and
//! the translation layer is a translation of what EI emits.

use std::collections::BTreeMap;

use crate::evtc::event::sc;
#[cfg(test)]
use crate::evtc::event::RawEvent;
use crate::evtc::guid::ContentType;
use crate::evtc::RawLog;

/// One glider deployment. `end_ms` is `None` when the glider was still
/// deployed at the last event in the log -- an open window is NOT closed at
/// log end and NOT dropped, because both of those are lies a consumer cannot
/// detect. (The two existing interval families in this project take the two
/// opposite guesses -- `replay`'s `dc` drops an open window, commander
/// segments close one -- and both had to be documented as deliberate
/// divergences. A third family gets to avoid the choice.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GliderInterval {
    /// The gliding agent's raw addr. Not folded across relog/build-swap
    /// addrs the way `Encounter::players` folds them: this pass runs on the
    /// raw event stream with no roster, and the join to an entity happens at
    /// the schema layer, which has the index.
    pub agent_addr: u64,
    /// Log-relative ms (`event.time - RawLog::log_start_ms()`), the
    /// project-wide convention.
    pub start_ms: u64,
    pub end_ms: Option<u64>,
}

/// One transformation window on one agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformationInterval {
    pub agent_addr: u64,
    /// The arcdps session-local transformation id (`skillid`). Never 0 --
    /// 0 is the untransform sentinel and closes an interval rather than
    /// opening one.
    pub transformation_id: u32,
    /// The stable content GUID for [`Self::transformation_id`], lowercase
    /// hex, resolved through `CBTS_IDTOGUID` with content type
    /// [`ContentType::Transformation`]. `None` when the log carries no
    /// mapping for this id, which is ordinary: the id is session-local and
    /// only useful across logs once resolved, so a consumer must be able to
    /// tell "unresolved" from "resolved to something".
    pub guid: Option<String>,
    pub start_ms: u64,
    pub end_ms: Option<u64>,
}

/// Both families, decoded in one pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentStates {
    /// In ascending start order, ties broken by first appearance in the
    /// event stream.
    pub gliding: Vec<GliderInterval>,
    pub transformations: Vec<TransformationInterval>,
}

impl AgentStates {
    pub fn is_empty(&self) -> bool {
        self.gliding.is_empty() && self.transformations.is_empty()
    }
}

/// Decode both families from a raw log.
pub fn build(raw: &RawLog) -> AgentStates {
    AgentStates { gliding: gliders(raw), transformations: transformations(raw) }
}

/// Glider deploy/stow windows.
///
/// The payload is one bit (`value == 1` deployed, `0` stowed -- GW2EI's
/// `GliderEvent.GliderDeployed` is literally `evtcItem.Value == 1`), so the
/// only decisions here are what to do with repeats and with an unclosed
/// window. A deploy while already gliding is IGNORED rather than treated as
/// a restart: arcdps documents the field as a status *change*, so a repeat
/// carries no new information, and restarting would emit a zero-length
/// interval plus a spurious second window for one continuous glide. A stow
/// with nothing open is likewise ignored -- it is the ordinary shape of a
/// log that started recording mid-glide, and inventing a window back to log
/// start would fabricate a deploy that was never observed.
pub fn gliders(raw: &RawLog) -> Vec<GliderInterval> {
    let t0 = raw.log_start_ms();
    let mut open: BTreeMap<u64, usize> = BTreeMap::new();
    let mut out: Vec<GliderInterval> = Vec::new();

    for e in raw.events.iter().filter(|e| e.is_statechange == sc::GLIDER) {
        let t = e.time.saturating_sub(t0);
        if e.value == 1 {
            if open.contains_key(&e.src_agent) {
                continue;
            }
            open.insert(e.src_agent, out.len());
            out.push(GliderInterval { agent_addr: e.src_agent, start_ms: t, end_ms: None });
        } else if let Some(i) = open.remove(&e.src_agent) {
            out[i].end_ms = Some(t);
        }
    }
    out
}

/// Transformation windows, with GW2EI's two closing rules ported exactly
/// (`CombatEventFactory.cs:525-550`):
///
/// 1. A row with `skillid == 0` (`TransformationEvent.IsEnd`) closes the
///    LAST transformation opened by that same agent -- not all of them, and
///    not by matching id.
/// 2. A row opening a NEW transformation force-closes that agent's previous
///    one at the new row's time, if it never got its own end row.
///
/// Both rules are per-`Src`, and EI's `SetEndTime` is write-once (a second
/// close on an already-closed window is a no-op), which is why rule 1 can
/// fire on a window rule 2 already closed without moving it.
pub fn transformations(raw: &RawLog) -> Vec<TransformationInterval> {
    let t0 = raw.log_start_ms();
    // Content-local id -> GUID, restricted to the transformation content
    // type. Restricting matters: the id space is per-content-type, so a
    // skill or marker id numerically equal to a transformation id would
    // otherwise resolve to the wrong GUID.
    let guids: BTreeMap<u32, String> = raw
        .guid_map
        .iter()
        .filter(|g| g.content_type == ContentType::Transformation)
        .map(|g| (g.local_id, g.guid_hex()))
        .collect();

    // Per-agent index of that agent's most recently OPENED window, whether
    // or not it is still open -- EI keeps a per-Src list and always looks at
    // `LastOrDefault()`, so a closed last window is still the one both rules
    // address (and the write-once `SetEndTime` is what makes that safe).
    let mut last: BTreeMap<u64, usize> = BTreeMap::new();
    let mut out: Vec<TransformationInterval> = Vec::new();

    for e in raw.events.iter().filter(|e| e.is_statechange == sc::TRANSFORMATION) {
        let t = e.time.saturating_sub(t0);
        let is_end = e.skillid == 0;
        match last.get(&e.src_agent).copied() {
            // Rule 1 and rule 2 are the same write-once close; they differ
            // only in whether a new window follows.
            Some(i) if out[i].end_ms.is_none() => out[i].end_ms = Some(t),
            _ => {}
        }
        if is_end {
            continue;
        }
        last.insert(e.src_agent, out.len());
        out.push(TransformationInterval {
            agent_addr: e.src_agent,
            transformation_id: e.skillid,
            guid: guids.get(&e.skillid).cloned(),
            start_ms: t,
            end_ms: None,
        });
    }
    out
}

/// Test-only helper mirroring the one in `crate::wvw::objectives`.
#[cfg(test)]
fn state_event(statechange: u8, time: u64, src_agent: u64, value: i32, skillid: u32) -> RawEvent {
    RawEvent {
        time,
        src_agent,
        dst_agent: 0,
        value,
        buff_dmg: 0,
        overstack: 0,
        skillid,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::guid::GuidMapping;
    use crate::evtc::RawHeader;

    fn log(events: Vec<RawEvent>, guid_map: Vec<GuidMapping>) -> RawLog {
        RawLog {
            header: RawHeader { build: String::new(), revision: 1, boss_id: 0 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map,
        }
    }

    fn glider(time: u64, src: u64, deployed: bool) -> RawEvent {
        super::state_event(sc::GLIDER, time, src, if deployed { 1 } else { 0 }, 0)
    }

    fn transform(time: u64, src: u64, id: u32) -> RawEvent {
        super::state_event(sc::TRANSFORMATION, time, src, 0, id)
    }

    /// The first event in the log is the time anchor, so every interval
    /// below is relative to it -- guarded here so a regression that reports
    /// raw arcdps session time (Phase B's worst defect, on a different
    /// family) cannot recur silently on this one.
    #[test]
    fn glider_windows_are_log_relative_and_pair_up() {
        let raw = log(
            vec![
                super::state_event(sc::MAP_ID, 1_000, 0, 0, 0),
                glider(1_500, 7, true),
                glider(3_200, 7, false),
                glider(4_000, 7, true),
            ],
            vec![],
        );
        let g = gliders(&raw);
        assert_eq!(
            g,
            vec![
                GliderInterval { agent_addr: 7, start_ms: 500, end_ms: Some(2_200) },
                // Still deployed at the last event: open, not closed and not
                // dropped.
                GliderInterval { agent_addr: 7, start_ms: 3_000, end_ms: None },
            ]
        );
    }

    /// A repeat deploy carries no new information (arcdps documents a status
    /// *change*), and a stow with nothing open is the ordinary shape of a log
    /// that started mid-glide. Neither may produce a window.
    #[test]
    fn repeat_deploys_and_orphan_stows_do_not_fabricate_windows() {
        let raw = log(
            vec![
                glider(0, 7, false), // started recording mid-glide
                glider(100, 7, true),
                glider(200, 7, true), // repeat
                glider(300, 7, false),
            ],
            vec![],
        );
        assert_eq!(
            gliders(&raw),
            vec![GliderInterval { agent_addr: 7, start_ms: 100, end_ms: Some(300) }]
        );
    }

    /// Two agents gliding at once must not close each other's windows.
    #[test]
    fn glider_windows_are_per_agent() {
        let raw = log(
            vec![
                glider(0, 7, true),
                glider(10, 8, true),
                glider(20, 7, false),
                glider(30, 8, false),
            ],
            vec![],
        );
        assert_eq!(
            gliders(&raw),
            vec![
                GliderInterval { agent_addr: 7, start_ms: 0, end_ms: Some(20) },
                GliderInterval { agent_addr: 8, start_ms: 10, end_ms: Some(30) },
            ]
        );
    }

    /// GW2EI's rule 2: a new transformation force-closes an unclosed
    /// previous one at the new row's time. Without this, back-to-back
    /// transformations both stay open forever.
    #[test]
    fn a_new_transformation_force_closes_the_previous_one() {
        let raw = log(vec![transform(0, 7, 100), transform(500, 7, 200)], vec![]);
        let t = transformations(&raw);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].transformation_id, t[0].start_ms, t[0].end_ms), (100, 0, Some(500)));
        assert_eq!((t[1].transformation_id, t[1].start_ms, t[1].end_ms), (200, 500, None));
    }

    /// GW2EI's rule 1: `skillid == 0` closes the last window and opens
    /// nothing. The id-0 row must never itself become an interval.
    #[test]
    fn skillid_zero_closes_and_never_opens() {
        let raw = log(vec![transform(0, 7, 100), transform(500, 7, 0)], vec![]);
        let t = transformations(&raw);
        assert_eq!(t.len(), 1, "the untransform sentinel is not a transformation");
        assert_eq!(t[0].end_ms, Some(500));
    }

    /// EI's `SetEndTime` is write-once. An untransform row arriving after a
    /// window was already force-closed must not move the end time.
    #[test]
    fn closing_an_already_closed_window_is_a_no_op() {
        let raw =
            log(vec![transform(0, 7, 100), transform(500, 7, 200), transform(600, 7, 0)], vec![]);
        let t = transformations(&raw);
        assert_eq!(t[0].end_ms, Some(500), "the force-close at 500 must survive the row at 600");
        assert_eq!(t[1].end_ms, Some(600));
    }

    /// The id is session-local, so the GUID join is the only thing that
    /// makes it portable -- and it must be restricted to the transformation
    /// content type, or a numerically equal skill/marker id resolves to the
    /// wrong GUID.
    #[test]
    fn transformation_guids_resolve_only_from_the_transformation_content_type() {
        let raw = log(
            vec![transform(0, 7, 100), transform(10, 8, 200)],
            vec![
                GuidMapping {
                    content_type: ContentType::Transformation,
                    local_id: 100,
                    guid: [0xab; 16],
                },
                // Same numeric id, wrong content type: must not be picked up
                // for transformation 200.
                GuidMapping { content_type: ContentType::Skill, local_id: 200, guid: [0xcd; 16] },
            ],
        );
        let t = transformations(&raw);
        assert_eq!(t[0].guid.as_deref(), Some("abababababababababababababababab"));
        assert_eq!(t[1].guid, None, "a Skill mapping must not resolve a Transformation id");
    }

    #[test]
    fn transformations_are_per_agent() {
        let raw = log(vec![transform(0, 7, 100), transform(10, 8, 200), transform(20, 7, 0)], vec![]);
        let t = transformations(&raw);
        assert_eq!(t[0].end_ms, Some(20), "agent 7's own untransform closes agent 7's window");
        assert_eq!(t[1].end_ms, None, "agent 8's window is untouched");
    }

    #[test]
    fn a_log_with_neither_family_is_empty() {
        let raw = log(vec![super::state_event(sc::MAP_ID, 0, 0, 0, 0)], vec![]);
        assert!(build(&raw).is_empty());
    }
}
