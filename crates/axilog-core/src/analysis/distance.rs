//! Distance scalars (Phase B, Task 7): GW2EI's `DistanceToCommander`
//! (exported as `statsAll[].distToCom`) and `DistanceToCenterOfSquad`
//! (`stackDist`), computed engine-side from the combat replay instead of
//! being re-derived by a consumer from the exported position arrays.
//!
//! ## Provenance
//!
//! Every rule below is read out of GW2EI source (checkout at
//! `/var/tmp/gw2ei`), not inferred from output:
//!
//! - `GW2EIEvtcParser/EIData/Statistics/GameplayStatistics.cs:29-69`
//!   (`GetDistanceToTarget`) -- the reduction itself, and lines 140-141,
//!   which pick the two references.
//! - `GW2EIEvtcParser/EIData/Statistics/StatisticsHelper.cs:201-257`
//!   (`CalculateStackCenterPositions`) -- the squad-centre reference.
//! - `GW2EIEvtcParser/EIData/Statistics/StatisticsHelper.cs:259-306`
//!   (`CalculateStackCommanderPositions`) -- the commander reference.
//! - `GW2EIEvtcParser/EIData/Statistics/StatisticsHelper.cs:308-383`
//!   (`CalculateCommanderStates`) + `EIData/Actors/Player.cs:101-115`
//!   (`Player.GetCommanderStates`) -- how one commander is resolved out of
//!   the several a WvW squad can have at once.
//! - `GW2EIEvtcParser/EIData/Actors/SingleActor.cs:265-295`
//!   (`GetCombatReplayActivePolledPositions`) +
//!   `EIData/Actors/ActorsHelper/SingleActorStatusHelper.cs:29-105`
//!   (`FillStatus`) -- what "active" means, boundaries included.
//!
//! ## The five rules
//!
//! 1. **Iterate the actor's ACTIVE polls only.** A poll spent down, dead or
//!    disconnected contributes nothing. See [`inactive_windows`] for the
//!    boundary rule, which is not the obvious one.
//! 2. **Pair by poll TIMESTAMP, never by index.** GW2EI walks the
//!    reference list with a monotonically advancing cursor and only records
//!    a distance on an exact `Time ==` hit (`GameplayStatistics.cs:52-58`);
//!    a reference that has no sample at the actor's poll time drops that
//!    pair rather than shifting the pairing. Both sequences are sorted by
//!    time, so an exact-key map lookup is equivalent to that cursor walk.
//! 3. **XY-plane length; Z is discarded** (`.XY().Length()`,
//!    `GameplayStatistics.cs:57`). A purely vertical separation is zero
//!    distance to GW2EI, however wrong that looks.
//! 4. **Arithmetic mean** over the qualifying pairs.
//! 5. **[`NO_DISTANCE`] (`-1`) when none qualified** -- GW2EI's own
//!    sentinel (`GameplayStatistics.cs:64`), and deliberately NOT zero:
//!    zero is a real, reachable distance (the commander's own `distToCom`
//!    is exactly `0`). Callers that need to distinguish "the replay pass
//!    never ran" from "it ran and nothing qualified" carry the first as
//!    absence and the second as this sentinel; see the schema's
//!    `ReplayIntervals::dist_to_com`.
//!
//! ## The two references, and why they are asymmetric
//!
//! - **Commander**: the commanding player's **raw** polled positions during
//!   their commander segments -- `CalculateStackCommanderPositions` calls
//!   `GetCombatReplayPolledPositions`, not the `Active` variant. A downed
//!   commander still anchors the squad.
//! - **Squad centre**: the per-poll mean of every squad player's **active**
//!   position -- `CalculateStackCenterPositions` calls
//!   `GetCombatReplayActivePolledPositions`.
//!
//! That asymmetry is GW2EI's, not a bug to harmonize away: matching it is
//! the difference between reproducing the statistic and merely
//! approximating it.
//!
//! ## Float arithmetic
//!
//! GW2EI computes all of this in `float` and only widens to `double` on
//! return, so this module mirrors the widths as well as the order: the
//! centre is summed and divided in `f32` (`Vector3` arithmetic), each
//! distance is an `f32` `sqrt`, and the mean is `Enumerable.Sum(this
//! IEnumerable<float>)`'s shape -- a `double` accumulator, narrowed to
//! `f32`, then divided by the count in `f32`. Computing the same reduction
//! in `f64` throughout produces a number that differs from GW2EI's in the
//! 7th significant figure, which at these magnitudes is ~1e-4 absolute:
//! visible against an exactness bar, invisible against a percentage one.

use crate::analysis::damage::is_health_damage_result;
use crate::analysis::replay::{Replay, Track};
use crate::evtc::{iff, RawLog};
use crate::model::Encounter;
use std::collections::{BTreeMap, BTreeSet};

/// GW2EI's sentinel for "the reduction ran and nothing qualified"
/// (`GameplayStatistics.GetDistanceToTarget`'s `distances.Count == 0 ? -1`).
pub const NO_DISTANCE: f64 = -1.0;

/// GW2EI's `ParserHelper.ServerDelayConstant` (10 ms), the gap under which
/// `CalculateCommanderStates` fuses two consecutive same-player, same-tag
/// commander segments into one.
const SERVER_DELAY_MS: i64 = 10;

/// One actor's two distance scalars, in world inches (the units the raw
/// `CBTS_POSITION` payload carries -- GW2EI's own map-pixel conversion
/// happens downstream of this statistic, not upstream of it).
///
/// Both fields use the [`NO_DISTANCE`] sentinel rather than an `Option`,
/// because that is the value GW2EI itself publishes and a consumer joining
/// against an EI export has to see the same number. The schema layer, which
/// additionally has to express "the replay pass never ran", is the one that
/// wraps these in `Option`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistanceScalars {
    pub dist_to_com: f64,
    pub stack_dist: f64,
}

impl Default for DistanceScalars {
    fn default() -> Self {
        DistanceScalars { dist_to_com: NO_DISTANCE, stack_dist: NO_DISTANCE }
    }
}

/// A reference timeline: poll timestamp -> `(x, y)`. Z is not carried,
/// because rule 3 discards it and carrying it would invite a reader to
/// believe it participates.
type Reference = BTreeMap<u64, (f32, f32)>;

/// Compute both scalars for every tracked actor, keyed by the track's
/// representative agent addr.
///
/// Takes the [`Encounter`] as well as the [`Replay`] because neither
/// reference can be built from position tracks alone: the squad-centre
/// roster and the commander candidates are both "players in the squad",
/// which is `Player::in_squad`, and the commander segments hang off
/// `Player::commander_tag`.
///
/// Takes the [`RawLog`] for two reasons that are not obvious:
///
/// - **The time base.** Replay samples are log-relative, while
///   `CommanderTag::segments` -- like `markers[].time_ms` beside it -- is
///   published in arcdps session time, a documented format convention
///   (`docs/NATIVE-FORMAT.md`: "same base as `markers[].time_ms` above --
///   arcdps session time, not encounter-relative"). The two have to meet
///   somewhere, and rebasing here is cheaper than re-basing a published
///   field.
/// - **Participation.** GW2EI's `PlayerList` is not "every squad member in
///   the agent table"; it is `activePlayers` (`ParsedEvtcLog.cs:70-105`),
///   which drops anyone who never interacted with the fight. See
///   [`participants`] -- on the committed fixture this is the difference
///   between a squad centre and a squad centre dragged 600 inches sideways
///   by one AFK player parked 22,000 inches away.
pub fn build(raw: &RawLog, replay: &Replay, enc: &Encounter) -> BTreeMap<u64, DistanceScalars> {
    let t0_ms = raw.log_start_ms();
    let active = participants(raw, enc);
    let squad = squad_tracks(replay, enc, &active);
    let centre = squad_centre(&squad, enc.duration_ms, replay.poll_ms);
    let commander = commander_positions(replay, enc, &active, t0_ms);

    let mut out = BTreeMap::new();
    for track in &replay.tracks {
        let inactive = inactive_windows(track);
        out.insert(
            track.agent_addr,
            DistanceScalars {
                dist_to_com: mean_distance(track, &inactive, &commander),
                stack_dist: mean_distance(track, &inactive, &centre),
            },
        );
    }
    out
}

/// The squad players GW2EI would keep in `PlayerList`, by representative
/// agent addr.
///
/// `ParsedEvtcLog.cs:70-99` keeps a player only if they actually took part:
/// they took damage from a non-friendly, dealt damage to a non-friendly,
/// applied a buff to somebody other than themselves, or one of their minions
/// dealt non-friendly damage. Everyone else is dropped with the log line
/// "Removing player from player list - did not participate".
///
/// This is not housekeeping -- it is load-bearing for the squad centre. The
/// committed fixture has exactly one such player, parked 22,000 inches from
/// the fight for its whole duration; including them in a 38-player mean
/// moves the centre ~600 inches, which is roughly four times the statistic
/// being measured.
///
/// **The damage predicate has a trap in it.** A buff APPLICATION row also
/// carries a `result` byte, and that byte is meaningless there -- reading it
/// as a damage result classifies "somebody applied a boon to me" as "I took
/// damage". GW2EI discriminates on the arcdps rule that a `buff != 0` row
/// with a non-zero `value` is an application (the value being a duration),
/// not damage. That one clause is what decides the fixture's AFK player.
///
/// **Three deliberate narrowings**, all flagged rather than hidden:
///
/// - The minion clause is not reproduced (it needs the instid-owner
///   registry, and a player whose ONLY contribution is minion damage while
///   never being hit, never hitting anything and never buffing anyone is a
///   case this project has never observed).
/// - The buff clause covers applications by this player onto another agent
///   rather than GW2EI's full `GetBuffDataBySrc` (which also spans
///   removals, whose `src`/`dst` roles are inverted).
/// - The damage predicate here goes through [`is_health_damage_result`],
///   which narrows GW2EI's own `GetDamageData`/`GetDamageTakenData` --
///   those are not result-filtered at all (`ParsedEvtcLog.cs:80-81`). A
///   player whose only hits were blocked or evaded is dropped by this gate
///   but kept by GW2EI's.
///
/// **Also not reproduced: GW2EI's two aware-window gates**
/// (`ParsedEvtcLog.cs:74,78`), evaluated before the participation predicate
/// above and not after it: `p.LastAware <= LogStart` removes a player who
/// despawned before the log started, and `p.FirstAware < LogEnd` removes
/// one who spawned after it ended (checked as `else`, i.e. `FirstAware >=
/// LogEnd`). Neither is exercised by the committed fixture.
fn participants(raw: &RawLog, enc: &Encounter) -> BTreeSet<u64> {
    let mut by_addr: BTreeMap<u64, u64> = BTreeMap::new();
    for p in &enc.players {
        if !p.in_squad {
            continue;
        }
        let rep = p.agent_addrs.first().copied().unwrap_or(p.agent_addr);
        for &a in &p.agent_addrs {
            by_addr.insert(a, rep);
        }
    }
    let mut out = BTreeSet::new();
    for e in &raw.events {
        if e.is_statechange != 0 || e.is_activation != 0 {
            continue;
        }
        let src = by_addr.get(&e.src_agent).copied();
        let dst = by_addr.get(&e.dst_agent).copied();
        if src.is_none() && dst.is_none() {
            continue;
        }
        let is_buff_application = e.buff != 0 && e.value != 0;
        let is_damage = e.is_buffremove == 0
            && !is_buff_application
            && is_health_damage_result(e.result)
            && e.iff != iff::FRIEND;
        if is_damage {
            out.extend(src);
            out.extend(dst);
        }
        if is_buff_application && e.is_buffremove == 0 && src.is_some() && src != dst {
            out.extend(src);
        }
    }
    out
}

/// The squad roster, in GW2EI's own `log.PlayerList` order.
///
/// Two things here are load-bearing rather than incidental:
///
/// - **Squad players only.** GW2EI's WvW logic puts friendly players who
///   are not in the recording squad into `_nonSquadFriendlies`
///   (`LogLogic/WvW/WvWLogic.cs:326-334`), never into `PlayerList`. They
///   still *receive* both scalars -- they are actors -- but they neither
///   move the squad centre nor supply a commander. `Player::in_squad` is
///   this project's own already-calibrated form of that split.
/// - **The order.** `PlayerList` is `activePlayers.OrderBy(a => a.Group)`
///   (`ParsedEvtcLog.cs:105`) over a list already ordered by character name
///   (`EvtcParser.cs:1051`); LINQ's `OrderBy` is stable, so the net order is
///   subgroup then character. That is reproduced here purely because the
///   centre is a float sum, and a float sum is order-dependent -- iterating
///   in agent-table order instead would perturb the last ulp of every
///   centre coordinate.
fn squad_tracks<'a>(
    replay: &'a Replay,
    enc: &Encounter,
    active: &BTreeSet<u64>,
) -> Vec<&'a Track> {
    let by_addr: BTreeMap<u64, &Track> =
        replay.tracks.iter().map(|t| (t.agent_addr, t)).collect();
    let mut roster: Vec<(u8, &str, &Track)> = Vec::new();
    for p in &enc.players {
        if !p.in_squad {
            continue;
        }
        let addr = p.agent_addrs.first().copied().unwrap_or(p.agent_addr);
        if !active.contains(&addr) {
            continue;
        }
        if let Some(track) = by_addr.get(&addr) {
            roster.push((p.subgroup, p.character.as_str(), track));
        }
    }
    roster.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    roster.into_iter().map(|(_, _, t)| t).collect()
}

/// The squad-centre reference: per poll, the arithmetic mean of every squad
/// player's ACTIVE position (`CalculateStackCenterPositions`). Polls where
/// nobody is active simply have no entry -- GW2EI's `null` -- and a `null`
/// reference drops the pair rather than contributing a zero.
///
/// **The centre stops one poll short of the last one, and that is not a
/// rounding artefact.** GW2EI sizes this reference at `sampleCount =
/// (int)(LogEnd / PollingRate)` and then emits it with `for (int t = 0; t <
/// sampleCount; t++)` (`StatisticsHelper.cs:213, 235`), so the highest
/// index it ever publishes is `sampleCount - 1`. On a 49,285 ms log at 300
/// ms that is index 163 (t = 48,900): every actor's own poll at t = 49,200
/// exists, is active, and finds no centre to pair with, so it drops. The
/// commander reference has no equivalent cap. Measured on the committed
/// fixture, against EI's own exported positions, omitting this cap is the
/// entire remaining error: 0.55% mean becomes 0.0017%, which is the
/// floor set by the fixture's 3-decimal pixel rounding.
fn squad_centre(squad: &[&Track], log_end_ms: u64, poll_ms: u64) -> Reference {
    let last_index = log_end_ms / poll_ms.max(1);
    let mut acc: BTreeMap<u64, (f32, f32, u32)> = BTreeMap::new();
    for track in squad {
        let inactive = inactive_windows(track);
        for s in &track.samples {
            if !is_active(s.t_ms, &inactive) {
                continue;
            }
            if s.t_ms / poll_ms.max(1) >= last_index {
                continue;
            }
            let slot = acc.entry(s.t_ms).or_insert((0.0, 0.0, 0));
            slot.0 += s.x;
            slot.1 += s.y;
            slot.2 += 1;
        }
    }
    acc.into_iter().map(|(t, (x, y, n))| (t, (x / n as f32, y / n as f32))).collect()
}

/// One resolved commander window: which track anchors the squad, and when.
struct TagSegment<'a> {
    start: u64,
    end: u64,
    addr: u64,
    guid: &'a str,
    track: &'a Track,
}

/// The commander reference: the commanding player's RAW polled positions
/// during the resolved commander windows.
///
/// Resolving *which* player is the reference at a given moment is the whole
/// difficulty, because a WvW squad routinely has several tags up at once.
/// GW2EI pools every squad player's segments, sorts by start time, and
/// applies **earliest tag-start wins** -- its own comment is "Overlap
/// protection, previous tag has priority" (`StatisticsHelper.cs:349-366`):
/// a later-starting segment that overlaps the incumbent is truncated to
/// begin where the incumbent ends. It is NOT latest-wins, NOT longest-tag,
/// and NOT iteration order. `CalculateStackCommanderPositions` then adds a
/// second, redundant guard of the same shape (the `start` watermark,
/// `StatisticsHelper.cs:280-302`), reproduced below.
///
/// Two deliberate simplifications against GW2EI, both no-ops on real data:
///
/// - The fuse rule compares segment tag GUIDs; this project stores one
///   resolved tag per player rather than one per segment, so the comparison
///   degenerates to "same player". A player who swapped tag colour across a
///   sub-10ms gap would be fused here and not by GW2EI -- a window narrower
///   than a thirtieth of one poll interval.
/// - `Player.GetCommanderStates` clamps each segment to that player's
///   first/last-aware. Skipped: a marker event cannot fall outside its own
///   agent's aware window, and the only end that can (an unclosed tag,
///   clamped to log end by `wvw::markers`) reaches past the player's last
///   position sample, where there is nothing to read anyway.
fn commander_positions(
    replay: &Replay,
    enc: &Encounter,
    active: &BTreeSet<u64>,
    t0_ms: u64,
) -> Reference {
    let by_addr: BTreeMap<u64, &Track> =
        replay.tracks.iter().map(|t| (t.agent_addr, t)).collect();

    let mut segments: Vec<TagSegment<'_>> = Vec::new();
    for p in &enc.players {
        if !p.in_squad {
            continue;
        }
        let Some(tag) = p.commander_tag.as_ref() else { continue };
        let addr = p.agent_addrs.first().copied().unwrap_or(p.agent_addr);
        if !active.contains(&addr) {
            continue;
        }
        let Some(track) = by_addr.get(&addr) else { continue };
        for &(start, end) in &tag.segments {
            let start = start.saturating_sub(t0_ms);
            let end = end.saturating_sub(t0_ms);
            segments.push(TagSegment { start, end, addr, guid: &tag.guid, track });
        }
    }
    segments.sort_by_key(|s| s.start);

    let mut resolved: Vec<TagSegment<'_>> = Vec::with_capacity(segments.len());
    let mut iter = segments.into_iter();
    let Some(mut last) = iter.next() else { return Reference::new() };
    for mut seg in iter {
        if last.end > seg.start {
            // Overlap protection: the incumbent keeps the overlap.
            seg.end = seg.end.max(last.end);
            seg.start = last.end;
        }
        if last.addr == seg.addr
            && last.guid == seg.guid
            && (last.end as i64 - seg.start as i64).abs() < SERVER_DELAY_MS
        {
            last.end = seg.end;
        } else {
            resolved.push(last);
            last = seg;
        }
    }
    resolved.push(last);
    resolved.retain(|s| s.start < s.end);

    let mut out = Reference::new();
    let mut watermark = 0u64;
    for seg in &resolved {
        for s in &seg.track.samples {
            if s.t_ms < watermark {
                continue;
            }
            if s.t_ms >= seg.end {
                break;
            }
            if s.t_ms >= seg.start {
                out.insert(s.t_ms, (s.x, s.y));
            }
        }
        watermark = seg.end;
    }
    out
}

/// The windows during which an actor's polls do NOT count, merged.
///
/// The boundary rule is the trap. GW2EI never tests "is this poll inside a
/// down window"; `SingleActorStatusHelper.FillStatus` builds its `actives`
/// list positively, straight from the alive-type status events
/// (`AddValueToStatusList`'s final `else` branch) -- it is not computed as
/// the complement of `down`/`dead`/`dc`. It happens to partition
/// `[FirstAware, LastAware]` with those three, which is what makes "active"
/// and "not in a merged down/dead/dc window" equivalent here, but the
/// mechanism GW2EI uses to get there is additive, not subtractive. GW2EI
/// then keeps a poll when `active.Start <= t <= active.End`
/// (`SingleActor.cs:283-295`) -- both ends inclusive. So a poll landing
/// exactly on a down/dead/dc boundary is ACTIVE, and only a poll strictly
/// inside a window is dropped.
///
/// Merging touching windows is what makes that equivalent rather than
/// merely similar: a player who goes down at 1000 and dies at 2000 has
/// `down = [1000, 2000)` and `dead = [2000, 3000)`, and GW2EI's active
/// segments simply skip from 1000 to 3000 -- a poll at exactly 2000 is
/// dead, not active, even though it is strictly inside neither window. It
/// IS strictly inside their merge.
fn inactive_windows(track: &Track) -> Vec<(u64, u64)> {
    let mut windows: Vec<(u64, u64)> = track
        .down_intervals
        .iter()
        .chain(track.dead_intervals.iter())
        .chain(track.dc_intervals.iter())
        .filter(|i| i.start_ms < i.end_ms)
        .map(|i| (i.start_ms, i.end_ms))
        .collect();
    windows.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(windows.len());
    for (start, end) in windows {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// See [`inactive_windows`] for why this is a strict, not a half-open, test.
fn is_active(t_ms: u64, inactive: &[(u64, u64)]) -> bool {
    !inactive.iter().any(|&(start, end)| start < t_ms && t_ms < end)
}

/// Rules 1-5, in GW2EI's own arithmetic widths and order.
fn mean_distance(track: &Track, inactive: &[(u64, u64)], reference: &Reference) -> f64 {
    if track.samples.is_empty() || reference.is_empty() {
        return NO_DISTANCE;
    }
    // `Enumerable.Sum(IEnumerable<float>)` accumulates in `double` and
    // narrows once, at the end -- not a running `float` sum.
    let mut sum: f64 = 0.0;
    let mut count: u32 = 0;
    for s in &track.samples {
        if !is_active(s.t_ms, inactive) {
            continue;
        }
        let Some(&(rx, ry)) = reference.get(&s.t_ms) else { continue };
        let dx = s.x - rx;
        let dy = s.y - ry;
        sum += f64::from((dx * dx + dy * dy).sqrt());
        count += 1;
    }
    if count == 0 {
        return NO_DISTANCE;
    }
    f64::from((sum as f32) / (count as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::replay::{Interval, Sample};
    use crate::evtc::{RawEvent, RawHeader};
    use crate::model::{CommanderTag, Player};

    /// A synthetic log in which every listed agent dealt one point of
    /// damage to a foe, so that `participants` keeps them. Position and
    /// status data live on the hand-built `Track`s instead; this exists
    /// only to satisfy GW2EI's participation gate, which is a property of
    /// the event stream and cannot be expressed on a `Track`.
    fn raw_with_participants(addrs: &[u64]) -> RawLog {
        let events = addrs
            .iter()
            .map(|&src| RawEvent {
                time: 0,
                src_agent: src,
                dst_agent: 999,
                value: 1,
                buff_dmg: 0,
                overstack: 0,
                skillid: 1,
                src_instid: 0,
                dst_instid: 0,
                src_master_instid: 0,
                dst_master_instid: 0,
                iff: iff::FOE,
                buff: 0,
                result: 0,
                is_activation: 0,
                is_buffremove: 0,
                is_ninety: 0,
                is_fifty: 0,
                is_moving: 0,
                is_statechange: 0,
                is_flanking: 0,
                is_shields: 0,
                is_offcycle: 0,
                pad: 0,
            })
            .collect();
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    /// Runs `build` over a hand-built replay, treating every squad player
    /// in `enc` as a participant.
    fn run(replay: &Replay, enc: &Encounter, addr: u64) -> DistanceScalars {
        let addrs: Vec<u64> = enc.players.iter().map(|p| p.agent_addr).collect();
        let raw = raw_with_participants(&addrs);
        build(&raw, replay, enc).remove(&addr).expect("the actor is tracked")
    }

    /// `(t_ms, x)` pairs at `y = z = 0`, the shape every case below uses
    /// for its 1-D geometry.
    fn track_x(addr: u64, name: &str, samples: &[(u64, f64)]) -> Track {
        Track {
            agent_addr: addr,
            name: name.into(),
            team: "red".into(),
            commander: false,
            is_squad: true,
            samples: samples
                .iter()
                .map(|&(t, x)| Sample { t_ms: t, x: x as f32, y: 0.0, z: 0.0 })
                .collect(),
            down_intervals: vec![],
            dead_intervals: vec![],
            dc_intervals: vec![],
        }
    }

    fn player(addr: u64, character: &str, in_squad: bool, tag: Option<Vec<(u64, u64)>>) -> Player {
        Player {
            agent_addr: addr,
            account: format!(":{character}.1"),
            character: character.into(),
            profession: "Thief".into(),
            elite_spec: String::new(),
            team: "red".into(),
            subgroup: if in_squad { 1 } else { 0 },
            in_squad,
            commander: tag.is_some(),
            marker: None,
            commander_tag: tag.map(|segments| CommanderTag {
                variant: "blue-commander".into(),
                guid: "guid".into(),
                segments,
            }),
            guild_id: None,
            agent_addrs: vec![addr],
        }
    }

    /// `build` reads only `poll_ms`/`tracks`; the `distance` map is its
    /// own output, so a hand-built replay starts with it empty.
    fn replay_of(tracks: Vec<Track>) -> Replay {
        Replay { poll_ms: 1000, tracks, distance: BTreeMap::new() }
    }

    fn encounter(players: Vec<Player>) -> Encounter {
        Encounter {
            kind: "wvw".into(),
            map: String::new(),
            duration_ms: 10_000,
            build: String::new(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players,
            enemies: vec![],
            markers: vec![], ground_markers: vec![],
            tick_rate: None, objectives: Vec::new(), started_at_unix: None, log_start_ms: 0, map_id: None,
        }
    }

    /// The workhorse: ONE squad player (who is both the commander and, being
    /// alone in the squad, the whole squad centre) plus a non-squad friendly
    /// actor whose scalars are returned. Keeping the actor out of the squad
    /// is what makes "distance to the centre" and "distance to the other
    /// guy" the same number -- an actor inside the roster would drag the
    /// centre halfway towards itself.
    fn run_two_actor_case(
        actor: &[(u64, f64)],
        reference: &[(u64, f64)],
        actor_down: &[(u64, u64)],
    ) -> DistanceScalars {
        let mut actor_track = track_x(1, "Actor", actor);
        actor_track.down_intervals = actor_down
            .iter()
            .map(|&(s, e)| Interval { start_ms: s, end_ms: e })
            .collect();
        let ref_track = track_x(2, "Ref", reference);
        let replay = replay_of(vec![actor_track, ref_track]);
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            player(2, "Ref", true, Some(vec![(0, 1_000_000)])),
        ]);
        run(&replay, &enc, 1)
    }

    /// EI iterates the actor's ACTIVE polls only -- positions are nulled
    /// while down, dead, or disconnected. A poll inside any of those
    /// windows must not contribute to the mean.
    #[test]
    fn excludes_polls_inside_down_dead_and_dc_windows() {
        // Actor at distance 100 for two polls, then down for a poll at
        // distance 1000. Mean must be 100, not 400.
        let scalars = run_two_actor_case(
            /* actor */ &[(0, 100.0), (1000, 100.0), (2000, 1000.0)],
            /* reference */ &[(0, 0.0), (1000, 0.0), (2000, 0.0)],
            /* actor_down */ &[(1500, 2500)],
        );
        assert_eq!(scalars.stack_dist, 100.0);
        assert_eq!(scalars.dist_to_com, 100.0);
    }

    /// The same exclusion, driven by `dead` and by `dc` rather than `down`
    /// -- all three feed one merged window list, and a regression that
    /// consulted only `down_intervals` would still pass the test above.
    #[test]
    fn dead_and_dc_windows_exclude_polls_the_same_way() {
        for which in 0..2 {
            let mut actor = track_x(1, "Actor", &[(0, 100.0), (1000, 100.0), (2000, 1000.0)]);
            let window = Interval { start_ms: 1500, end_ms: 2500 };
            if which == 0 {
                actor.dead_intervals = vec![window];
            } else {
                actor.dc_intervals = vec![window];
            }
            let reference = track_x(2, "Ref", &[(0, 0.0), (1000, 0.0), (2000, 0.0)]);
            let replay = replay_of(vec![actor, reference]);
            let enc = encounter(vec![
                player(1, "Actor", false, None),
                player(2, "Ref", true, Some(vec![(0, 1_000_000)])),
            ]);
            let scalars = run(&replay, &enc, 1);
            assert_eq!(scalars.stack_dist, 100.0, "window kind {which}");
        }
    }

    /// A poll landing exactly on a window boundary is ACTIVE (GW2EI's
    /// active segments are closed at both ends), but a poll at the junction
    /// of two touching windows is not. See `inactive_windows`.
    #[test]
    fn boundary_polls_are_active_but_junction_polls_are_not() {
        let mut actor = track_x(1, "Actor", &[(1000, 100.0), (2000, 900.0), (3000, 500.0)]);
        actor.down_intervals = vec![Interval { start_ms: 1000, end_ms: 2000 }];
        actor.dead_intervals = vec![Interval { start_ms: 2000, end_ms: 3000 }];
        let reference = track_x(2, "Ref", &[(1000, 0.0), (2000, 0.0), (3000, 0.0)]);
        let replay = replay_of(vec![actor, reference]);
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            player(2, "Ref", true, Some(vec![(0, 1_000_000)])),
        ]);
        let scalars = run(&replay, &enc, 1);
        // t=1000 (down starts) and t=3000 (dead ends) are kept; t=2000, the
        // junction of the two touching windows, is dropped: (100+500)/2.
        // A half-open `[start, end)` reading would drop t=1000 too and
        // report 500; ignoring the merge would keep t=2000 and report 500.
        assert_eq!(scalars.dist_to_com, 300.0);

        let mut only_junction = track_x(1, "Actor", &[(2000, 900.0)]);
        only_junction.down_intervals = vec![Interval { start_ms: 1000, end_ms: 2000 }];
        only_junction.dead_intervals = vec![Interval { start_ms: 2000, end_ms: 3000 }];
        let reference = track_x(2, "Ref", &[(2000, 0.0)]);
        let replay = replay_of(vec![only_junction, reference]);
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            player(2, "Ref", true, Some(vec![(0, 1_000_000)])),
        ]);
        let scalars = run(&replay, &enc, 1);
        assert_eq!(scalars.dist_to_com, NO_DISTANCE, "the junction poll is not active");
    }

    /// Distance is XY-plane only; Z is discarded. A pure vertical
    /// separation is zero distance to EI.
    #[test]
    fn discards_the_z_axis() {
        let scalars = run_vertical_separation_case(/* dz */ 5000.0);
        assert_eq!(scalars.stack_dist, 0.0);
        assert_eq!(scalars.dist_to_com, 0.0);
    }

    fn run_vertical_separation_case(dz: f64) -> DistanceScalars {
        let mut actor = track_x(1, "Actor", &[(0, 0.0), (1000, 0.0)]);
        for s in &mut actor.samples {
            s.z = dz as f32;
        }
        let reference = track_x(2, "Ref", &[(0, 0.0), (1000, 0.0)]);
        let replay = replay_of(vec![actor, reference]);
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            player(2, "Ref", true, Some(vec![(0, 1_000_000)])),
        ]);
        run(&replay, &enc, 1)
    }

    /// Polls pair by TIMESTAMP, not by index. A reference missing a poll
    /// the actor has must drop that pair rather than shift the pairing.
    #[test]
    fn pairs_by_timestamp_not_index() {
        let scalars = run_two_actor_case(
            &[(0, 100.0), (1000, 100.0), (2000, 100.0)],
            &[(0, 0.0), (2000, 0.0)], // reference missing t=1000
            &[],
        );
        assert_eq!(scalars.stack_dist, 100.0, "the unmatched poll drops, it does not shift");
    }

    /// The index-pairing failure mode the test above cannot see: with a
    /// constant separation, a shifted pairing still yields 100. Vary the
    /// actor's position so a one-slot shift produces a different mean.
    #[test]
    fn pairing_by_index_would_produce_a_different_mean() {
        let scalars = run_two_actor_case(
            &[(0, 0.0), (1000, 600.0), (2000, 300.0)],
            &[(0, 0.0), (2000, 0.0)], // reference missing t=1000
            &[],
        );
        // Timestamp pairing: |0-0| and |300-0| -> mean 150.
        // Index pairing would take actor[1] (600) against reference[1] -> 300.
        assert_eq!(scalars.stack_dist, 150.0);
    }

    /// EI's sentinel for "nothing qualified" is -1, NOT zero and NOT
    /// absent. Absence means the replay pass never ran; -1 means it ran
    /// and this actor had no qualifying poll.
    #[test]
    fn reports_minus_one_when_no_poll_qualifies() {
        let scalars = run_two_actor_case(
            &[(0, 100.0)],
            &[(5000, 0.0)], // no overlapping timestamp at all
            &[],
        );
        assert_eq!(scalars.stack_dist, -1.0);
        assert_eq!(scalars.dist_to_com, -1.0);
    }

    /// The commander reference uses the commanding player's RAW positions
    /// during their segments -- NOT active-filtered. The squad centre uses
    /// every player's ACTIVE position. GW2EI's two references differ this
    /// way on purpose. On the fixture that certifies the reduction exactly
    /// (`distance_semantics_match_ei_exactly`), this particular asymmetry
    /// measures zero effect -- no squad member there goes down while
    /// tagged -- so it is not one the exactness gate would itself catch
    /// dropping; this test, and the GW2EI source reading, are what pin it.
    #[test]
    fn commander_reference_is_raw_but_squad_centre_is_active() {
        let scalars = run_downed_commander_case();
        assert_ne!(
            scalars.dist_to_com, -1.0,
            "a downed commander still provides a reference position"
        );
        assert_eq!(scalars.dist_to_com, 100.0);
        assert_eq!(
            scalars.stack_dist, -1.0,
            "but the squad centre has no active player to average"
        );
    }

    fn run_downed_commander_case() -> DistanceScalars {
        let actor = track_x(1, "Actor", &[(1000, 100.0), (2000, 100.0)]);
        let mut commander = track_x(2, "Comm", &[(1000, 0.0), (2000, 0.0)]);
        // Down strictly across every poll the commander has -- the window
        // has to overhang both, since a poll on the boundary stays active.
        commander.down_intervals = vec![Interval { start_ms: 500, end_ms: 5000 }];
        let replay = replay_of(vec![actor, commander]);
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            player(2, "Comm", true, Some(vec![(0, 1_000_000)])),
        ]);
        run(&replay, &enc, 1)
    }

    /// Two overlapping tags: the one that STARTED EARLIEST owns the
    /// overlap, and the later one only takes over where the earlier ends
    /// (`CalculateCommanderStates`' "Overlap protection, previous tag has
    /// priority"). Constructed so latest-wins and earliest-wins give
    /// different means rather than merely different segment lists.
    #[test]
    fn the_earliest_starting_commander_tag_owns_the_overlap() {
        let actor = track_x(1, "Actor", &[(0, 0.0), (1000, 0.0), (2000, 0.0), (3000, 0.0), (4000, 0.0)]);
        let early = track_x(2, "Early", &[(0, 0.0), (1000, 0.0), (2000, 0.0), (3000, 0.0), (4000, 0.0)]);
        let late = track_x(
            3,
            "Late",
            &[(0, 1000.0), (1000, 1000.0), (2000, 1000.0), (3000, 1000.0), (4000, 1000.0)],
        );
        let replay = replay_of(vec![actor, early, late]);
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            // "Early" tags at 0 and drops at 3000; "Late" tags at 1000 --
            // inside Early's window -- and holds to 5000.
            player(2, "Early", true, Some(vec![(0, 3000)])),
            player(3, "Late", true, Some(vec![(1000, 5000)])),
        ]);
        let scalars = run(&replay, &enc, 1);
        // Earliest wins: Early anchors t=0,1000,2000 (distance 0), Late
        // anchors t=3000,4000 (distance 1000) -> 2000/5 = 400.
        //
        // Latest wins would give Early only t=0 and Late t=1000..4000 ->
        // 4000/5 = 800. This test discriminates earliest-wins from
        // latest-wins only -- it does not by itself discriminate against
        // removing the overlap truncation above and relying solely on the
        // `watermark` guard in `commander_positions`: that guard
        // independently enforces earliest-wins on this fixture too (GW2EI
        // has the same second, redundant guard), so with truncation
        // removed this assertion still passes.
        assert_eq!(scalars.dist_to_com, 400.0);
    }

    /// A friendly player outside the squad neither anchors the centre nor
    /// can be the commander: GW2EI's `PlayerList` is squad-only.
    #[test]
    fn a_non_squad_friendly_player_does_not_move_the_squad_centre() {
        let actor = track_x(1, "Actor", &[(0, 100.0)]);
        let squad = track_x(2, "Squad", &[(0, 0.0)]);
        // A pug standing far away, in the log but not in the squad.
        let pug = track_x(3, "Pug", &[(0, 10_000.0)]);
        let replay = replay_of(vec![actor, squad, pug]);
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            player(2, "Squad", true, Some(vec![(0, 1_000_000)])),
            player(3, "Pug", false, Some(vec![(0, 1_000_000)])),
        ]);
        let scalars = run(&replay, &enc, 1);
        assert_eq!(scalars.stack_dist, 100.0, "the pug is not in the centre");
        assert_eq!(scalars.dist_to_com, 100.0, "and cannot be the commander");
    }

    /// The squad centre really is a mean over the roster, not the first
    /// player's position.
    #[test]
    fn the_squad_centre_is_the_mean_of_the_roster() {
        let actor = track_x(1, "Actor", &[(0, 0.0)]);
        let a = track_x(2, "Aaa", &[(0, 100.0)]);
        let b = track_x(3, "Bbb", &[(0, 300.0)]);
        let replay = replay_of(vec![actor, a, b]);
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            player(2, "Aaa", true, None),
            player(3, "Bbb", true, None),
        ]);
        let scalars = run(&replay, &enc, 1);
        assert_eq!(scalars.stack_dist, 200.0);
        assert_eq!(scalars.dist_to_com, NO_DISTANCE, "nobody tagged up");
    }

    /// The squad centre is published only for poll indices below
    /// `LogEnd / poll_ms`, so an actor's very last poll has nothing to pair
    /// with even though the actor is active and every squad member has a
    /// position there. See `squad_centre`.
    #[test]
    fn the_squad_centre_stops_one_poll_short_of_the_log_end() {
        let actor = track_x(1, "Actor", &[(9000, 100.0), (10000, 700.0)]);
        let squad = track_x(2, "Squad", &[(9000, 0.0), (10000, 0.0)]);
        let replay = replay_of(vec![actor, squad]);
        // `encounter` fixes `duration_ms` at 10_000 and `replay_of` at a
        // 1000 ms poll, so index 10 (t = 10_000) is the first one GW2EI
        // does not publish.
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            player(2, "Squad", true, Some(vec![(0, 1_000_000)])),
        ]);
        let scalars = run(&replay, &enc, 1);
        assert_eq!(scalars.stack_dist, 100.0, "only t=9000 pairs with a centre");
        assert_eq!(
            scalars.dist_to_com, 400.0,
            "the commander reference has no such cap, so both polls pair"
        );
    }

    /// The commander's own `distToCom` is exactly 0 -- the reference is
    /// their own track, so `dx`/`dy` are exactly zero regardless of what
    /// the position values are, and no position pipeline sits in the way.
    /// That makes it real, but not an arithmetic check: this quantity can
    /// only ever come out as `0.0` (a commander segment resolved at least
    /// one qualifying poll) or `-1.0` (it did not), so what it actually
    /// asserts is that commander-segment resolution found a poll -- it is
    /// why the sentinel is -1 and not 0, not a check of the distance math.
    #[test]
    fn the_commanders_own_distance_to_the_commander_is_zero() {
        let commander = track_x(2, "Comm", &[(0, 500.0), (1000, 700.0)]);
        let replay = replay_of(vec![commander]);
        let enc = encounter(vec![player(2, "Comm", true, Some(vec![(0, 1_000_000)]))]);
        let scalars = run(&replay, &enc, 2);
        assert_eq!(scalars.dist_to_com, 0.0);
        assert_eq!(scalars.stack_dist, 0.0, "alone in the squad, they are the centre");
    }

    /// GW2EI's mean narrows through `f32` before the final division
    /// (`Enumerable.Sum(IEnumerable<float>)`'s own shape: a `double`
    /// accumulator, narrowed to `f32`, THEN divided by the count in `f32`).
    /// Fix-round-1 pin: this was previously an unenforced claim -- computing
    /// the same reduction in plain `f64` throughout survives every existing
    /// test with zero failures. Three exact distances (1.0, 2.0, 4.0
    /// world-inches, chosen so `sqrt` is exact in `f32`) sum to a value
    /// whose `f32`-narrowed-then-divided mean differs from the `f64`
    /// pure-division mean at the 7th significant figure -- confirmed with a
    /// throwaway program mirroring both formulas: narrowed =
    /// `2.3333332538604736`, pure `f64` = `2.3333333333333335`. This test
    /// asserts the narrowed value; changing `mean_distance` to
    /// `sum / count as f64` fails it (verified by hand while writing this
    /// test, then reverted -- see the fix-round-1 report for the observed
    /// failure).
    #[test]
    fn the_mean_narrows_through_f32_before_dividing_by_the_count() {
        let scalars = run_two_actor_case(
            &[(0, 1.0), (1000, 2.0), (2000, 4.0)],
            &[(0, 0.0), (1000, 0.0), (2000, 0.0)],
            &[],
        );
        assert_eq!(scalars.stack_dist, 2.3333332538604736);
        assert_eq!(scalars.dist_to_com, 2.3333332538604736);
    }

    /// The squad-centre sum runs in GW2EI's own `PlayerList` order --
    /// subgroup, then character name (`squad_tracks`) -- because a `f32`
    /// sum is order-dependent. Fix-round-1 pin: this was previously an
    /// unenforced claim -- reversing the roster summation order survives
    /// every existing test with zero failures, because none of them choose
    /// magnitudes far enough apart for `f32` addition to lose a term.
    /// These three positions are chosen so that DOES happen: summing
    /// ascending (Alpha, Bravo, Charlie) computes `(-1e20 + 1e20) + 1.0 =
    /// 1.0`, but summing descending (Charlie, Bravo, Alpha) computes
    /// `(1.0 + 1e20) + -1e20 = 0.0` -- the `1.0` term is lost to rounding
    /// the moment it is added before the `1e20`/`-1e20` pair cancels.
    /// Confirmed with a throwaway program mirroring `squad_centre` and
    /// `mean_distance`: ascending gives `0.3333333432674408`, descending
    /// gives `0.0`. This test asserts the ascending (subgroup, character)
    /// value; reversing the sort in `squad_tracks` fails it (verified by
    /// hand while writing this test, then reverted -- see the fix-round-1
    /// report for the observed failure).
    #[test]
    fn the_squad_centre_sums_the_roster_in_subgroup_then_character_order() {
        let actor = track_x(1, "Actor", &[(0, 0.0)]);
        let alpha = track_x(2, "Alpha", &[(0, -1.0e20)]);
        let bravo = track_x(3, "Bravo", &[(0, 1.0e20)]);
        let charlie = track_x(4, "Charlie", &[(0, 1.0)]);
        let replay = replay_of(vec![actor, alpha, bravo, charlie]);
        let mk = |addr: u64, character: &str, subgroup: u8| Player {
            agent_addr: addr,
            account: format!(":{character}.1"),
            character: character.into(),
            profession: "Thief".into(),
            elite_spec: String::new(),
            team: "red".into(),
            subgroup,
            in_squad: true,
            commander: false,
            marker: None,
            commander_tag: None,
            guild_id: None,
            agent_addrs: vec![addr],
        };
        let enc = encounter(vec![
            player(1, "Actor", false, None),
            mk(2, "Alpha", 1),
            mk(3, "Bravo", 1),
            mk(4, "Charlie", 2),
        ]);
        let scalars = run(&replay, &enc, 1);
        assert_eq!(
            scalars.stack_dist, 0.3333333432674408,
            "ascending (subgroup, character) order: Alpha, Bravo, Charlie"
        );
    }
}
