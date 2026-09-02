//! Enemy attention per squad player -- who the other side is actually
//! aiming at.
//!
//! # Why this is possible at all
//!
//! arcdps's enemy-event filter records a non-squad agent's events only when
//! that agent interacts with the recording squad. For `CBTS_ANIMATIONSTART`
//! that filter is DST-DRIVEN: an enemy cast-start row survives into the log
//! exactly when its `dst_agent` is a squad member. So the surviving rows are
//! not a sample of enemy activity -- they are a census of enemy activity
//! *pointed at us*, with the target attached. That is the whole signal here.
//!
//! Two consequences of the same filter, both load-bearing:
//!
//! - There is no matching `CBTS_ANIMATIONSTOP` census. Those rows carry the
//!   caster as `src` and nothing squad-side as `dst`, so the filter drops
//!   them. No cast durations, no interrupt detection, no channel tracking --
//!   only starts.
//! - Untargeted enemy casts are invisible. A ground-targeted AoE dropped on
//!   the squad's position emits no row here. This pass measures *aimed*
//!   attention specifically, not incoming pressure in general;
//!   [`crate::analysis::defenses`] already covers what actually landed.
//!
//! # What was measured
//!
//! Against ~1,400 real WvW logs (2026-09-01), using the commander as the
//! known-focused player and [`PlayerFocus::focus_index`] as the yardstick,
//! across three disjoint slices:
//!
//! | slice | logs | commander | median other |
//! |---|---|---|---|
//! | A | 592 | 1.92x | 0.56x |
//! | B | 196 | 1.86x | 0.53x |
//! | C | 582 | 1.47x | 0.64x |
//!
//! A commander draws roughly three times the aimed attention of a median
//! squad member, consistently, on every slice.
//!
//! # Why there is no threat weighting, despite the obvious appeal
//!
//! The first cut of this pass weighted each cast by how hard its skill hits
//! (self-calibrated from the log, shrunk toward a 300-log corpus table of 60
//! measured hard hitters) on the theory that a raw cast count is dominated by
//! autoattacks and therefore measures proximity as much as intent. An offline
//! study on 300 logs supported that: in the 3s before a down, hard-hitter
//! casts separated the commander 2.05x against 1.50x for casts in general.
//!
//! It did not survive holdout. On unseen logs the weighted index was never
//! better than simply counting casts, on any slice:
//!
//! | slice | weighted index | unweighted | weighted pre-down lift | unweighted |
//! |---|---|---|---|---|
//! | A | 1.87x | **1.92x** | 1.51x | **1.58x** |
//! | B | 1.86x | 1.86x | **1.65x** | 1.52x |
//! | C | 1.40x | **1.47x** | 1.41x | 1.37x |
//!
//! -- so the weighting, the shrinkage, and the corpus table were all removed
//! rather than shipped as decoration. The 2.05x was a property of the 300
//! logs it was measured on. This note exists so the idea is not re-derived
//! from scratch: it is a reasonable hypothesis that has already been tested
//! and did not hold.
//!
//! CC weighting was rejected on the same evidence and one structural ground:
//! a CC row names the EFFECT and discards the CAUSE (arcdps substitutes its
//! own generic control ids -- see [`crate::analysis::control_catalog`]), so
//! the cast behind it must be recovered by back-matching to the most recent
//! cast-start by the same enemy, and in the study set that left 66% of
//! incoming CC rows unattributable. Instant casts emit no `ANIMATIONSTART` at
//! all, and AoE control lands on players who were never the cast's target.
//!
//! [`SkillThreat`] survives as a DIAGNOSTIC -- "what is being aimed at me,
//! and how hard does it hit when it connects" is a useful breakdown even
//! though scoring with it is not.
//!
//! Indexing is POSITIONAL over [`Encounter::players`], matching
//! [`crate::analysis::entity_series`]. Non-squad friendlies get a zeroed
//! [`PlayerFocus`]: the enemy filter is defined relative to the RECORDING
//! squad, so a pug's incoming casts are not in the log to begin with, and a
//! nonzero row for them would be a lie about coverage, not a measurement.

use std::collections::{BTreeMap, BTreeSet};

use crate::evtc::{event::sc, result, RawLog};
use crate::model::Encounter;

/// How far back from a down to attribute incoming casts. 3s is short enough
/// that the casts inside it plausibly caused the down rather than merely
/// preceding it, and long enough to contain the cast->travel->hit chain of a
/// ranged burst.
pub const PRE_DOWN_WINDOW_MS: u64 = 3000;

/// One enemy skill's aimed-cast count and measured hitting power.
///
/// DIAGNOSTIC ONLY -- nothing in [`PlayerFocus`] is scored with these; see
/// the module doc for the holdout that decided that. Present because a
/// "what is being aimed at me" breakdown is worth surfacing on its own.
#[derive(Debug, Clone)]
pub struct SkillThreat {
    pub skill_id: u32,
    /// Cast-starts aimed at any squad member.
    pub casts_at_squad: u64,
    /// Enemy->squad direct strike hits observed for this skill. `0` is normal
    /// and means the skill was aimed at us but never connected as direct
    /// damage -- pure CC, pure support, or a whiffed cast.
    pub hits: u64,
    /// Total enemy->squad strike damage. Carried alongside [`Self::hits`]
    /// rather than only the mean because the two are what a multi-log
    /// consumer needs to POOL a session: means cannot be averaged across
    /// logs, `(hits, damage_total)` pairs can be summed. This matters --
    /// the MEDIAN enemy skill connects just 3 times in a single log.
    pub damage_total: u64,
    /// Mean damage per connecting hit. `0.0` when `hits == 0`.
    pub mean_damage: f64,
}

/// Enemy attention drawn by one squad player.
#[derive(Debug, Clone, Default)]
pub struct PlayerFocus {
    /// Enemy cast-starts naming this player as target.
    pub casts_drawn: u64,
    /// This player's share of squad-wide [`Self::casts_drawn`], as a multiple
    /// of an even 1/N split. `1.0` is a fair share; `2.0` is twice the
    /// attention a uniformly-targeted squad would put on one player.
    ///
    /// `0.0` for a non-squad friendly, and for every player when the log
    /// carries no enemy casts at all -- both are "not measured", not "not
    /// focused". Check [`FocusDetail::total_casts`] to tell them apart.
    pub focus_index: f64,
    /// `CBTS_CHANGEDOWN` transitions for this player. The denominator for
    /// [`Self::pre_down_casts`].
    pub downs: u64,
    /// Incoming aimed casts inside [`PRE_DOWN_WINDOW_MS`] before each of this
    /// player's downs. Windows are NOT deduplicated across rapid successive
    /// downs: a cast preceding two downs 1s apart is counted for both,
    /// because it is evidence about both.
    pub pre_down_casts: u64,
}

#[derive(Debug, Clone, Default)]
pub struct FocusDetail {
    per_player: Vec<PlayerFocus>,
    /// Squad members in the [`PlayerFocus::focus_index`] denominator
    /// (`in_squad` players). The fair share is `1.0 / squad_size`.
    pub squad_size: usize,
    /// Enemy cast-starts aimed at any squad member, across the whole log.
    pub total_casts: u64,
    /// Mean enemy->squad direct strike damage. Diagnostic context for
    /// [`SkillThreat::mean_damage`] -- roughly 900 in the study corpus.
    pub mean_strike_damage: f64,
    /// Per-skill diagnostics, for skills that were cast at the squad.
    pub skills: BTreeMap<u32, SkillThreat>,
}

impl FocusDetail {
    pub fn len(&self) -> usize { self.per_player.len() }
    pub fn is_empty(&self) -> bool { self.per_player.is_empty() }
    pub fn at(&self, i: usize) -> &PlayerFocus { &self.per_player[i] }
    pub fn iter(&self) -> impl Iterator<Item = &PlayerFocus> { self.per_player.iter() }
}

/// Build the pass from a resolved encounter and its raw log.
///
/// Two scans over `raw.events`: one to collect the aimed casts and the
/// per-skill damage diagnostics, one to walk downs back over the collected
/// casts. Cheap relative to the decode that produced `raw`, but not free, and
/// nothing else in the pipeline needs it -- so like `missiles`/
/// `entity_series` this is standalone and the GATE stays at the call site.
pub fn build(enc: &Encounter, raw: &RawLog) -> FocusDetail {
    // A friendly account can own several raw agent addrs (relog / build swap
    // mid-recording), so membership is the union of all of them while
    // `addr_to_idx` folds each back onto one positional slot -- the same
    // shape `entity_series::build_from` derives, for the same reason.
    let mut squad: BTreeSet<u64> = BTreeSet::new();
    let mut addr_to_idx: BTreeMap<u64, usize> = BTreeMap::new();
    let mut squad_size = 0usize;
    for (i, p) in enc.players.iter().enumerate() {
        if !p.in_squad { continue }
        squad_size += 1;
        for &a in &p.agent_addrs {
            squad.insert(a);
            addr_to_idx.insert(a, i);
        }
    }
    // Enemy PLAYERS only. NPCs (siege, guards, veterans) aim at the squad
    // too, but they do not choose targets the way a player does, so folding
    // them in would dilute the very thing being measured.
    let enemy: BTreeSet<u64> = enc
        .enemies
        .iter()
        .filter(|e| e.is_player)
        .flat_map(|e| e.agent_addrs.iter().copied())
        .collect();

    let mut out = FocusDetail {
        per_player: vec![PlayerFocus::default(); enc.players.len()],
        squad_size,
        ..Default::default()
    };
    if squad.is_empty() || enemy.is_empty() { return out }

    // (time, target addr) for every enemy cast aimed at us, in event order --
    // retained because the pre-down pass needs to look backwards from each
    // down, and `raw.events` is not indexed by target.
    let mut casts: Vec<(u64, u64)> = Vec::new();
    let mut dmg: BTreeMap<u32, (u64, u64)> = BTreeMap::new();
    let mut dmg_hits: u64 = 0;
    let mut dmg_total: u64 = 0;

    for e in &raw.events {
        if e.is_statechange == sc::ANIMATION_START
            && enemy.contains(&e.src_agent)
            && squad.contains(&e.dst_agent)
        {
            casts.push((e.time, e.dst_agent));
            if let Some(&i) = addr_to_idx.get(&e.dst_agent) {
                out.per_player[i].casts_drawn += 1;
            }
            out.skills
                .entry(e.skillid)
                .or_insert(SkillThreat {
                    skill_id: e.skillid, casts_at_squad: 0, hits: 0, damage_total: 0,
                    mean_damage: 0.0,
                })
                .casts_at_squad += 1;
            continue;
        }
        // Direct strike damage, enemy player -> squad member. Excludes buff
        // (condition) ticks, whose `skillid` is the CONDITION rather than the
        // skill that applied it, and excludes the two non-health results --
        // the same exclusion `damage::is_health_damage_result` enforces.
        if e.is_statechange == 0
            && e.buff == 0
            && e.is_buffremove == 0
            && e.result != result::CROWD_CONTROL
            && e.result != result::BREAKBAR_DAMAGE
            && e.value > 0
            && enemy.contains(&e.src_agent)
            && squad.contains(&e.dst_agent)
        {
            let d = e.value as u64;
            let slot = dmg.entry(e.skillid).or_default();
            slot.0 += 1;
            slot.1 += d;
            dmg_hits += 1;
            dmg_total += d;
        }
    }

    out.total_casts = casts.len() as u64;
    out.mean_strike_damage =
        if dmg_hits == 0 { 0.0 } else { dmg_total as f64 / dmg_hits as f64 };

    // Fold damage onto the skills that were AIMED at us. A skill that dealt
    // damage but was never aimed at anyone (untargeted AoE that still
    // connected) deliberately gets no entry: it is real incoming damage and
    // counts toward `mean_strike_damage`, but there is no aimed cast to
    // attach it to and inventing one would misstate what this pass covers.
    for (skill_id, (hits, total)) in dmg {
        if let Some(t) = out.skills.get_mut(&skill_id) {
            t.hits = hits;
            t.damage_total = total;
            t.mean_damage = if hits == 0 { 0.0 } else { total as f64 / hits as f64 };
        }
    }

    // Pre-down windows. `casts` is in event order, so a backwards scan can
    // stop as soon as it walks out of the window.
    for e in &raw.events {
        if e.is_statechange != sc::CHANGE_DOWN { continue }
        let Some(&i) = addr_to_idx.get(&e.src_agent) else { continue };
        out.per_player[i].downs += 1;
        let lo = e.time.saturating_sub(PRE_DOWN_WINDOW_MS);
        for &(t, dst) in casts.iter().rev() {
            if t > e.time { continue }
            if t < lo { break }
            if dst == e.src_agent { out.per_player[i].pre_down_casts += 1 }
        }
    }

    // Focus index last: it needs the squad-wide total, so it cannot be folded
    // into the accumulation above.
    let total: u64 = out.per_player.iter().map(|f| f.casts_drawn).sum();
    if total > 0 && squad_size > 0 {
        for f in &mut out.per_player {
            f.focus_index = (f.casts_drawn as f64 / total as f64) * squad_size as f64;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};
    use crate::model::{Enemy, Encounter, Player};

    fn base_event() -> RawEvent {
        RawEvent {
            time: 0, src_agent: 0, dst_agent: 0, value: 0, buff_dmg: 0, overstack: 0,
            skillid: 0, src_instid: 0, dst_instid: 0, src_master_instid: 0,
            dst_master_instid: 0, iff: 1, buff: 0, result: 0, is_activation: 0,
            is_buffremove: 0, is_ninety: 0, is_fifty: 0, is_moving: 0, is_statechange: 0,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn cast(time: u64, src: u64, dst: u64, skill: u32) -> RawEvent {
        let mut e = base_event();
        e.time = time; e.src_agent = src; e.dst_agent = dst; e.skillid = skill;
        e.is_statechange = sc::ANIMATION_START;
        e
    }

    fn hit(time: u64, src: u64, dst: u64, skill: u32, dmg: i32) -> RawEvent {
        let mut e = base_event();
        e.time = time; e.src_agent = src; e.dst_agent = dst; e.skillid = skill; e.value = dmg;
        e
    }

    fn down(time: u64, who: u64) -> RawEvent {
        let mut e = base_event();
        e.time = time; e.src_agent = who; e.is_statechange = sc::CHANGE_DOWN;
        e
    }

    fn player(addr: u64, in_squad: bool) -> Player {
        Player {
            agent_addr: addr, account: format!(":P{addr}.0001"), character: format!("P{addr}"),
            profession: "Guardian".into(), elite_spec: "".into(), team: "red".into(),
            subgroup: 1, in_squad, commander: false, marker: None, commander_tag: None,
            guild_id: None, agent_addrs: vec![addr],
        }
    }

    fn enemy(addr: u64) -> Enemy {
        Enemy {
            id: addr, instid: addr as u16, name: format!("E{addr}"), team: "blue".into(),
            is_player: true, marker: None, profession: Some("Necromancer".into()),
            elite_spec: Some("".into()), agent_addrs: vec![addr],
        }
    }

    fn enc_of(players: Vec<Player>, enemies: Vec<Enemy>) -> Encounter {
        Encounter {
            kind: "wvw".into(), pve: None, map: "".into(), duration_ms: 60_000,
            build: "".into(), revision: 1, recorded_by: None, teams: vec![], players,
            enemies, markers: vec![], ground_markers: vec![], tick_rate: None,
            objectives: Vec::new(), started_at_unix: None, log_start_ms: 0, map_id: None,
        }
    }

    fn log_of(mut events: Vec<RawEvent>) -> RawLog {
        events.sort_by_key(|e| e.time);
        RawLog {
            header: RawHeader { build: "".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills: vec![], events, guid_map: Default::default(),
        }
    }

    /// Two squad players, one enemy. P100 is aimed at 3x, P101 once, against
    /// a 1/2 fair split.
    #[test]
    fn focus_index_is_share_over_fair_share() {
        let enc = enc_of(vec![player(100, true), player(101, true)], vec![enemy(200)]);
        let raw = log_of(vec![
            cast(1000, 200, 100, 9), cast(2000, 200, 100, 9), cast(3000, 200, 100, 9),
            cast(4000, 200, 101, 9),
        ]);
        let d = build(&enc, &raw);
        assert_eq!(d.squad_size, 2);
        assert_eq!(d.total_casts, 4);
        assert_eq!(d.at(0).casts_drawn, 3);
        assert_eq!(d.at(1).casts_drawn, 1);
        assert!((d.at(0).focus_index - 1.5).abs() < 1e-9, "{}", d.at(0).focus_index);
        assert!((d.at(1).focus_index - 0.5).abs() < 1e-9);
    }

    /// A log with enemies but no aimed casts must leave every index at 0.0
    /// ("not measured") rather than dividing by zero into NaN.
    #[test]
    fn no_casts_leaves_indices_at_zero() {
        let enc = enc_of(vec![player(100, true), player(101, true)], vec![enemy(200)]);
        let d = build(&enc, &log_of(vec![hit(1000, 200, 100, 9, 500)]));
        assert_eq!(d.total_casts, 0);
        assert!(d.at(0).focus_index.is_finite());
        assert!((d.at(0).focus_index - 0.0).abs() < 1e-9);
    }

    /// Pre-down windows are per-target and bounded; a cast just outside the
    /// window, or aimed at somebody else, must not leak in.
    #[test]
    fn pre_down_window_is_bounded_and_targeted() {
        let enc = enc_of(vec![player(100, true), player(101, true)], vec![enemy(200)]);
        let raw = log_of(vec![
            cast(1_000, 200, 100, 9),  // 9s before the down -- outside
            cast(8_000, 200, 100, 9),  // inside
            cast(8_500, 200, 101, 9),  // inside the window, aimed elsewhere
            cast(9_500, 200, 100, 9),  // inside
            down(10_000, 100),
        ]);
        let d = build(&enc, &raw);
        assert_eq!(d.at(0).downs, 1);
        assert_eq!(d.at(0).casts_drawn, 3);
        assert_eq!(d.at(0).pre_down_casts, 2);
        assert_eq!(d.at(1).downs, 0);
        assert_eq!(d.at(1).pre_down_casts, 0);
    }

    /// Overlapping windows from two rapid downs both count the same cast --
    /// it is evidence about both, and deduplicating would understate the
    /// second down.
    #[test]
    fn overlapping_pre_down_windows_both_count() {
        let enc = enc_of(vec![player(100, true)], vec![enemy(200)]);
        let raw = log_of(vec![
            cast(5_000, 200, 100, 9), down(6_000, 100), down(7_000, 100),
        ]);
        let d = build(&enc, &raw);
        assert_eq!(d.at(0).downs, 2);
        assert_eq!(d.at(0).pre_down_casts, 2);
    }

    /// Non-squad friendlies are outside the enemy filter's coverage, so their
    /// row must stay zeroed and they must not enter the denominator.
    #[test]
    fn non_squad_friendly_is_zeroed_and_excluded() {
        let enc = enc_of(
            vec![player(100, true), player(101, true), player(102, false)],
            vec![enemy(200)],
        );
        let raw = log_of(vec![cast(1000, 200, 100, 9), cast(2000, 200, 102, 9)]);
        let d = build(&enc, &raw);
        assert_eq!(d.squad_size, 2);
        assert_eq!(d.at(2).casts_drawn, 0);
        assert!((d.at(2).focus_index - 0.0).abs() < 1e-9);
        // P100 drew the only counted cast: the whole squad share, over a 1/2
        // fair split.
        assert!((d.at(0).focus_index - 2.0).abs() < 1e-9, "{}", d.at(0).focus_index);
    }

    /// Enemy NPCs do not choose targets the way players do, so their casts
    /// must not enter the census.
    #[test]
    fn enemy_npc_casts_are_excluded() {
        let mut npc = enemy(300);
        npc.is_player = false;
        let enc = enc_of(vec![player(100, true)], vec![enemy(200), npc]);
        let raw = log_of(vec![cast(1000, 200, 100, 9), cast(2000, 300, 100, 9)]);
        let d = build(&enc, &raw);
        assert_eq!(d.total_casts, 1);
    }

    /// Damage diagnostics attach only to skills that were AIMED at us, and
    /// exclude condition ticks (whose id is the condition, not the skill) and
    /// the two non-health results.
    #[test]
    fn damage_diagnostics_exclude_ticks_and_non_health_results() {
        let enc = enc_of(vec![player(100, true)], vec![enemy(200)]);
        let mut tick = hit(1500, 200, 100, 9, 9999);
        tick.buff = 1;
        let mut cc = hit(1600, 200, 100, 9, 3000);
        cc.result = result::CROWD_CONTROL;
        let mut bb = hit(1700, 200, 100, 9, 4000);
        bb.result = result::BREAKBAR_DAMAGE;
        let raw = log_of(vec![
            cast(1000, 200, 100, 9), hit(1100, 200, 100, 9, 1000), tick, cc, bb,
            // Damage from a skill that was never aimed at anyone.
            hit(1800, 200, 100, 77, 5000),
        ]);
        let d = build(&enc, &raw);
        assert_eq!(d.skills[&9].hits, 1);
        assert!((d.skills[&9].mean_damage - 1000.0).abs() < 1e-9);
        assert!(!d.skills.contains_key(&77), "unaimed skill must not get a row");
        // ...but it still counts toward the log's mean strike: (1000+5000)/2.
        assert!((d.mean_strike_damage - 3000.0).abs() < 1e-9, "{}", d.mean_strike_damage);
    }

    /// A skill aimed at us that never connected keeps a cast row with zero
    /// damage, rather than being dropped.
    #[test]
    fn aimed_but_undamaging_skill_keeps_a_row() {
        let enc = enc_of(vec![player(100, true)], vec![enemy(200)]);
        let d = build(&enc, &log_of(vec![cast(1000, 200, 100, 7)]));
        assert_eq!(d.skills[&7].casts_at_squad, 1);
        assert_eq!(d.skills[&7].hits, 0);
        assert!((d.skills[&7].mean_damage - 0.0).abs() < 1e-9);
    }

    /// A relogged player's second agent addr must fold onto the same slot,
    /// not vanish and not double-count the squad denominator.
    #[test]
    fn relog_addrs_fold_onto_one_slot() {
        let mut p = player(100, true);
        p.agent_addrs = vec![100, 150];
        let enc = enc_of(vec![p, player(101, true)], vec![enemy(200)]);
        let raw = log_of(vec![
            cast(1000, 200, 100, 9), cast(2000, 200, 150, 9), cast(3000, 200, 101, 9),
            down(4000, 150),
        ]);
        let d = build(&enc, &raw);
        assert_eq!(d.squad_size, 2);
        assert_eq!(d.at(0).casts_drawn, 2);
        assert_eq!(d.at(0).downs, 1);
    }
}
