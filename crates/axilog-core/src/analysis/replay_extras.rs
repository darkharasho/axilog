//! The three replay eye-candy families, bundled into one pass.
//!
//! [`agent_states`], [`gadget_capture`] and [`decorations`] are independent
//! and individually callable; this exists so the ordinary caller -- the CLI
//! and both SDKs -- makes one call and threads one borrow, rather than three
//! of each. It adds no semantics of its own.
//!
//! **Always-on, not gated.** Like `replay::build_activity_intervals`, this is
//! cheap by construction: two filtered scans for the agent-state families,
//! and a capture assembly that exits before touching the event stream at all
//! on any log written before arcdps build `20260602` (which is every log
//! predating the capture family, i.e. almost all of them today). There is
//! nothing here worth a flag.
//!
//! [`agent_states`]: crate::analysis::agent_states
//! [`gadget_capture`]: crate::analysis::gadget_capture
//! [`decorations`]: crate::analysis::decorations

use crate::analysis::agent_states::{self, AgentStates};
use crate::analysis::decorations::{self, Decoration};
use crate::analysis::gadget_capture::{self, GadgetCapture};
use crate::evtc::RawLog;

/// Everything the replay eye-candy families produce for one log.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplayExtras {
    /// Glider and transformation windows, per agent.
    pub agent_states: AgentStates,
    /// Capture-point areas, as decoded -- owner and progress timelines.
    pub captures: Vec<GadgetCapture>,
    /// The renderable projection of [`Self::captures`].
    ///
    /// Carried ALONGSIDE the decode rather than instead of it, even though
    /// it is fully derived. The two answer different questions and neither
    /// reconstructs the other: a decoration has lost which owner index it
    /// came from (it carries a colour, and the four-value wrbg space is not
    /// recoverable from an `rgba` string once `Owner::Unknown` folds into
    /// white), while the decode has no lifespan resolution, no anchor and no
    /// geometry relative to it. A consumer analysing who held what and for
    /// how long wants the former; a consumer drawing a map wants the latter.
    pub decorations: Vec<Decoration>,
}

impl ReplayExtras {
    pub fn is_empty(&self) -> bool {
        self.agent_states.is_empty() && self.captures.is_empty() && self.decorations.is_empty()
    }
}

/// Run all three families over a decoded log.
pub fn build(raw: &RawLog) -> ReplayExtras {
    let captures = gadget_capture::build(raw);
    let decorations = decorations::build_environment_decorations(raw, &captures);
    ReplayExtras { agent_states: agent_states::build(raw), captures, decorations }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::event::sc;
    use crate::evtc::{RawEvent, RawHeader};

    fn glider(time: u64, src: u64, deployed: bool) -> RawEvent {
        RawEvent {
            time,
            src_agent: src,
            dst_agent: 0,
            value: if deployed { 1 } else { 0 },
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
            is_statechange: sc::GLIDER,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
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

    #[test]
    fn a_log_with_none_of_the_three_families_is_empty() {
        assert!(build(&log(vec![])).is_empty());
    }

    /// One family present is enough to make the bundle non-empty -- guarding
    /// against an `is_empty` that ANDs the wrong way and reports a log with
    /// real glider data as having nothing.
    #[test]
    fn one_family_alone_makes_the_bundle_non_empty() {
        let extras = build(&log(vec![glider(0, 7, true), glider(100, 7, false)]));
        assert!(!extras.is_empty());
        assert_eq!(extras.agent_states.gliding.len(), 1);
        assert!(extras.captures.is_empty());
        assert!(extras.decorations.is_empty());
    }
}
