//! Opt-in missile (projectile) analytics (M10 Task 2): per-squad-player
//! `fired`/`hit`/`denied` counts plus a squad-wide "incoming, denied"
//! defensive-effectiveness rollup. Standalone from
//! [`crate::analysis::analyze`] -- `Metrics` does not grow by default; a
//! caller opts in by calling [`build_missiles`] separately (mirrors M9
//! Task 1's `analysis::replay::build_replay`), CLI wiring is `--missiles`.
//!
//! ## Ordinal verification (project law: curl + hand-count the arcdps
//! README enum, never `WebFetch` it -- a prior quick grep for these SAME
//! ordinals produced numbers that were provably wrong, e.g. claiming 46 was
//! `CBTS_EFFECT` when `IDTOGUID=46` is independently triple-verified
//! elsewhere in this file)
//!
//! `curl https://www.deltaconnected.com/arcdps/evtc/README.txt` (2026-08-09),
//! a full line-by-line hand-count of every `CBTS_*` enumerator in `enum
//! cbtstatechange` from `CBTS_COMBAT = 0` (86 enumerators total, ending in
//! `CBTS_UNKNOWN`):
//!
//! ```text
//!  0 COMBAT           18 BUFFINITIAL       37 MARKER              56 STUNBREAK
//!  1 ENTERCOMBAT      19 POSITION          38 BARRIERPCTUPDATE    57 MISSILECREATE   <-- verified
//!  2 EXITCOMBAT       20 VELOCITY          39 STATRESET_DEFUNC    58 MISSILELAUNCH   <-- verified
//!  3 CHANGEUP         21 FACING            40 EXTENSION           59 MISSILEREMOVE   <-- verified
//!  4 CHANGEDEAD       22 TEAMCHANGE        41 APIDELAYED_DEFUNC   60 EFFECTGROUNDCREATE
//!  5 CHANGEDOWN       23 ATTACKTARGET      42 INSTANCESTART       61 EFFECTGROUNDREMOVE
//!  6 SPAWN            24 TARGETABLE        43 RATEHEALTH          62 EFFECTAGENTCREATE
//!  7 DESPAWN          25 MAPID             44 LAST90BEFOREDOWN..  63 EFFECTAGENTREMOVE
//!  8 HEALTHPCTUPDATE  26 REPLINFO          45 EFFECT1_DEFUNC      64 IIDCHANGE
//!  9 SQCOMBATSTART    27 BUFFACTIVE        46 IDTOGUID (verified) 65 MAPCHANGE
//! 10 SQCOMBATEND      28 BUFFDEACTIVE      47 LOGNPCUPDATE        66 EARLYEXIT
//! 11 WEAPSWAP         29 GUILD             48 IDLEEVENT           67 ANIMATIONSTART
//! 12 MAXHEALTHUPDATE  30 BUFFINFO          49 EXTENSIONCOMBAT     68 ANIMATIONSTOP
//! 13 POINTOFVIEW      31 BUFFFORMULA       50 FRACTALSCALE        69 BUFFAPPLY
//! 14 LANGUAGE         32 SKILLINFO         51 EFFECT2_DEFUNC      70 BUFFCHANGE
//! 15 GWBUILD          33 SKILLTIMING       52 RULESET             71 BUFFREMOVE_SINGLE
//! 16 SHARDID          34 DEFIANCEBARSTATE  53 SQUADMARKER_GROUND  72 BUFFREMOVE_ALL
//! 17 REWARD           35 DEFIANCEBARPCT    54 ARCBUILD            73 TRANSFORMATION
//!                     36 INTEGRITY         55 GLIDER              74 WVWTEAMS (verified)
//! 75 WVWOBJECTIVESTATUS  79 MISSILEEFFECT (verified)  83 GADGETCAPTUREOUTLINEPOINT
//! 76 STEALTHCHANGE       80 GADGETCAPTUREOUTLINESHOW  84 TICK (verified)
//! 77 GADGETANIMATION     81 GADGETCAPTURESPLITPERCENT 85 UNKNOWN
//! 78 GADGETNAME          82 GADGETCAPTUREOUTLINEHIDE
//! ```
//!
//! Every project-law anchor lands exactly where already independently
//! verified elsewhere in this crate: `IDTOGUID=46`, `STUNBREAK=56`,
//! `ANIMATIONSTART=67`, `BUFFAPPLY=69`, `WVWTEAMS=74`, `TICK=84`,
//! `EXTENSION=40`, `EXTENSIONCOMBAT=49`, `POSITION=19`. Cross-checked
//! against GW2EI's `ArcDPSEnums.StateChange` (`/tmp/gw2ei`,
//! `GW2EIEvtcParser/ParserHelpers/ArcDPSEnums.cs:317-319,339`):
//! `MissileCreate=57`, `MissileLaunch=58`, `MissileRemove=59`,
//! `EffectMissileCreate=79` -- exact agreement on all four. See
//! `crate::evtc::event::sc::MISSILE_CREATE`/`MISSILE_LAUNCH`/
//! `MISSILE_REMOVE`/`MISSILE_EFFECT`'s doc comments for the per-ordinal
//! payload citations.
//!
//! ## CRITICAL payload finding: no wire-level "denial reason" or "denier"
//! field exists
//!
//! The task brief anticipated a remove-reason field distinguishing
//! "blocked" vs "reflected" vs "destroyed" vs "expired naturally", plus an
//! agent field identifying who caused the denial. Neither exists.
//! `CBTS_MISSILEREMOVE`'s ENTIRE outcome signal, per the arcdps reference
//! text, is `is_src_flanking: hit at least one enemy along the way` -- a
//! plain hit/no-hit boolean. GW2EI's own `MissileRemoveEvent` ctor
//! (`ParsedData/CombatEvents/StatusEvents/MissileEvents/MissileRemoveEvent.cs`)
//! confirms this precisely: it decodes `DidHit = evtcItem.IsFlanking > 0`,
//! `FriendlyFireTotalDamage = evtcItem.Value`, and an optional
//! `DamagingAgent` from `src_agent` -- no reason/cause enum anywhere.
//! `DamagingAgent` is registered into GW2EI's own `MissileDamagingEventsBySrc`
//! map ONLY when `DidHit` is true (`CombatEventFactory.cs:483-486`) -- i.e.
//! GW2EI's own model reads that field as "who this missile damaged on a
//! SUCCESSFUL hit", not "who denied it on a miss" -- and that map has no
//! other callers anywhere in GW2EI's codebase (dead/speculative API
//! surface, never wired into any real statistic). There is consequently no
//! usable "denier" identity for a missile that did NOT hit.
//!
//! What follows from this, precisely:
//! - **`fired`** (attributable, exact): one per `CBTS_MISSILECREATE`,
//!   credited to the missile's owner (`src_agent`).
//! - **`hit`** (attributable, exact): the owning missile's
//!   `CBTS_MISSILEREMOVE.is_src_flanking != 0`.
//! - **`denied`** (attributable, but DELIBERATELY undifferentiated): the
//!   owning missile's `CBTS_MISSILEREMOVE.is_src_flanking == 0` -- the
//!   missile was created and resolved without hitting anything. This
//!   combines blocked/reflected/destroyed/expired-out-of-range/missed an
//!   evading target into one bucket because the wire format does not
//!   distinguish them -- reporting a fabricated 3-way split here would be
//!   actively misleading (implying certainty about a "0 blocks" count that
//!   is really "unknown"), which is worse than not splitting at all.
//! - **`reflected_at_self`** (a heuristic, explicitly labeled as such): GW2EI
//!   derives `MaybeReflected` (their own "Maybe"-prefixed, i.e.
//!   NOT-certain, naming) from a DIFFERENT signal -- `CBTS_MISSILELAUNCH`,
//!   which CAN repeat for the same trackable id (a relaunch/bounce), each
//!   carrying `is_src_flanking: non-zero if first launch`. When a relaunch
//!   (`is_src_flanking == 0`) targets (`dst_agent`) the missile's OWN
//!   original owner, the missile is heading back at its own caster --
//!   consistent with (but not proof of) a reflect effect. This project
//!   reproduces that exact heuristic, attributed to the missile's OWNER as
//!   the party who got their own missile turned around on them (a victim
//!   stat, not a denier-credit stat -- the relaunch event does NOT identify
//!   WHO caused the bounce; `iff`/`is_buffremove` on `CBTS_MISSILELAUNCH`
//!   are both documented by arcdps itself as "unknown, from client").
//! - **No per-player denier credit is computed anywhere in this module.**
//!   The squad-level `incoming_denied` rollup below is the closest
//!   defensible substitute: an AGGREGATE count, not attributed to any one
//!   squad member.
//!
//! ## Squad-level "incoming, denied" (defensive effectiveness)
//!
//! For every missile instance whose owner is NOT a squad address and whose
//! `CBTS_MISSILELAUNCH` target (`dst_agent`, when set) IS a squad address,
//! this counts as one `incoming_fired`; if that same instance's remove
//! event resolves with `hit == false`, it additionally counts as one
//! `incoming_denied` -- i.e. "an enemy-owned missile aimed at the squad
//! that did not land", aggregated across the whole squad with no per-player
//! attribution (per the finding above). An instance with NO remove event
//! at all (e.g. still in flight when the log ends) counts toward neither
//! `hit` nor `denied` anywhere in this module -- an unresolved outcome is
//! not claimed as a resolved one.
//!
//! Known, accepted scope limitation: "owner not in squad" is used directly
//! as the incoming-hostile signal, without cross-checking WvW team/`iff` --
//! matches this project's existing missile-agnostic conventions (e.g.
//! `analysis::replay`'s squad/enemy split) and is correct for the
//! overwhelming majority of real missiles (enemy player/NPC skills), but
//! could in principle misclassify a rare edge case like the squad's own
//! siege/gadget-owned projectile arcing near a squad member. Not chased
//! further -- out of scope per "document precisely what's attributable; do
//! not stretch".
//!
//! ## Missile-instance correlation (`pad`/trackable id)
//!
//! `CBTS_MISSILECREATE`/`CBTS_MISSILELAUNCH`/`CBTS_MISSILEREMOVE` all carry
//! the SAME trackable id in `pad` (`pad61` per the arcdps reference).
//! Reproduces GW2EI's own `CombatEventFactory` correlation exactly
//! (`MissileEventsByTrackingID`, `LastOrDefault(x => x.Time <= T)`): a
//! single forward pass over already-time-ordered `raw.events` (the same
//! ordering assumption every other per-event pass in this crate already
//! relies on -- see e.g. `analysis::replay::build_intervals`) tracks, per
//! trackable id, the index of the MOST RECENT `CBTS_MISSILECREATE` seen so
//! far; a `CBTS_MISSILELAUNCH`/`CBTS_MISSILEREMOVE` attaches to whichever
//! instance is currently open for its trackable id. A later
//! `CBTS_MISSILECREATE` reusing the same trackable id (arcdps recycles ids,
//! same caveat as instid reuse elsewhere in this crate) correctly starts a
//! NEW instance without any special-case cleanup, since forward-chronological
//! processing means "most recent CREATE so far" is always the right one to
//! attach to.
//!
//! ## Account folding
//!
//! Squad-owned missile instances are credited to a squad player's
//! representative `agent_addr`, folding across relog/build-swap addrs the
//! same way `analysis::analyze`/`analysis::replay` do (`Player::agent_addrs`).
//! Deliberately NOT folded from pet/minion agents onto their owner (unlike
//! `analysis::damage`/`analysis::cc`/`analysis::healing`'s
//! `src_master_instid`-based owner resolution) -- out of this task's scope;
//! a pet/minion/turret's own missile fire is credited to nobody (not
//! double-counted, but not attributed to its owner either). Documented here
//! as a known limitation rather than silently guessed at.

use crate::evtc::{sc, RawLog};
use crate::model::Encounter;
use std::collections::BTreeMap;

/// One squad player's missile totals (account-folded). Always present for
/// every `Encounter::players` entry, all-zero when that player fired no
/// missiles -- mirrors `analysis::replay::Track` always having one entry
/// per roster member.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayerMissiles {
    pub agent_addr: u64,
    /// Count of `CBTS_MISSILECREATE` events owned by this player.
    pub fired: u32,
    /// Of `fired`, how many resolved with `CBTS_MISSILEREMOVE.is_src_flanking
    /// != 0` ("hit at least one enemy along the way").
    pub hit: u32,
    /// Of `fired`, how many resolved with `is_src_flanking == 0` --
    /// deliberately undifferentiated (blocked/reflected/destroyed/expired
    /// are indistinguishable on the wire; see this module's doc comment).
    pub denied: u32,
    /// Heuristic count of this player's OWN fired missiles that got
    /// relaunched back at them (GW2EI's `MaybeReflected` signal --
    /// see this module's doc comment). A victim-side stat, not a
    /// denier-credit stat.
    pub reflected_at_self: u32,
}

/// Squad-wide missile totals: the sum of every [`PlayerMissiles`] entry,
/// plus the aggregate, unattributed "incoming, denied" defensive rollup
/// (see this module's doc comment).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SquadMissiles {
    pub fired: u32,
    pub hit: u32,
    pub denied: u32,
    /// Enemy-owned missile instances whose launch targeted a squad member
    /// at least once.
    pub incoming_fired: u32,
    /// Of `incoming_fired`, how many resolved without hitting anything --
    /// aggregate only, no per-player attribution (see this module's doc
    /// comment for why).
    pub incoming_denied: u32,
}

#[derive(Debug, Clone)]
pub struct Missiles {
    pub players: Vec<PlayerMissiles>,
    pub squad: SquadMissiles,
}

/// One correlated missile instance, built up across its
/// CREATE/LAUNCH*/REMOVE? event sequence (see this module's doc comment for
/// the trackable-id correlation algorithm).
struct MissileInstance {
    owner: u64,
    /// `Some(true)`/`Some(false)` once a `CBTS_MISSILEREMOVE` resolves this
    /// instance; `None` if it never resolves (e.g. still in flight at log
    /// end) -- an unresolved instance counts toward neither `hit` nor
    /// `denied`.
    hit: Option<bool>,
    /// Set when any `CBTS_MISSILELAUNCH` for this instance targeted
    /// (`dst_agent`) a squad address while `owner` is NOT itself a squad
    /// address (the squad-level "incoming" signal).
    targeted_squad: bool,
    /// GW2EI's `MaybeReflected` heuristic: a non-first launch
    /// (`is_src_flanking == 0`) whose target is this instance's own
    /// `owner`.
    reflected_at_owner: bool,
}

/// Build [`Missiles`] from a decoded log. Standalone -- does not read or
/// alter anything [`crate::analysis::analyze`] computes; a caller that
/// wants both simply calls both (mirrors `analysis::replay::build_replay`).
pub fn build_missiles(raw: &RawLog, enc: &Encounter) -> Missiles {
    let squad: std::collections::BTreeSet<u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().copied())
        .collect();
    let addr_to_rep: BTreeMap<u64, u64> = enc
        .players
        .iter()
        .flat_map(|p| p.agent_addrs.iter().map(move |&a| (a, p.agent_addr)))
        .collect();

    let mut instances: Vec<MissileInstance> = Vec::new();
    // trackable id (`pad`) -> index of the most recently created instance
    // for that id, per this module's doc comment.
    let mut open: BTreeMap<u32, usize> = BTreeMap::new();

    for e in &raw.events {
        match e.is_statechange {
            sc::MISSILE_CREATE => {
                instances.push(MissileInstance {
                    owner: e.src_agent,
                    hit: None,
                    targeted_squad: false,
                    reflected_at_owner: false,
                });
                open.insert(e.pad, instances.len() - 1);
            }
            sc::MISSILE_LAUNCH => {
                if let Some(&idx) = open.get(&e.pad) {
                    let target = e.dst_agent; // 0 == unset/out of range, per the arcdps reference
                    if target != 0 {
                        let owner = instances[idx].owner;
                        let is_first_launch = e.is_flanking != 0;
                        if squad.contains(&target) && !squad.contains(&owner) {
                            instances[idx].targeted_squad = true;
                        }
                        if !is_first_launch && target == owner {
                            instances[idx].reflected_at_owner = true;
                        }
                    }
                }
            }
            sc::MISSILE_REMOVE => {
                if let Some(&idx) = open.get(&e.pad) {
                    instances[idx].hit = Some(e.is_flanking != 0);
                }
            }
            _ => {}
        }
    }

    let mut by_rep: BTreeMap<u64, PlayerMissiles> = BTreeMap::new();
    let mut squad_totals = SquadMissiles::default();

    for inst in &instances {
        if squad.contains(&inst.owner) {
            squad_totals.fired += 1;
            match inst.hit {
                Some(true) => squad_totals.hit += 1,
                Some(false) => squad_totals.denied += 1,
                None => {}
            }
        }
        if let Some(&rep) = addr_to_rep.get(&inst.owner) {
            let agg = by_rep.entry(rep).or_insert(PlayerMissiles { agent_addr: rep, ..Default::default() });
            agg.fired += 1;
            match inst.hit {
                Some(true) => agg.hit += 1,
                Some(false) => agg.denied += 1,
                None => {}
            }
            if inst.reflected_at_owner {
                agg.reflected_at_self += 1;
            }
        }
        if inst.targeted_squad {
            squad_totals.incoming_fired += 1;
            if inst.hit == Some(false) {
                squad_totals.incoming_denied += 1;
            }
        }
    }

    let players = enc
        .players
        .iter()
        .map(|p| {
            by_rep.get(&p.agent_addr).copied().unwrap_or(PlayerMissiles {
                agent_addr: p.agent_addr,
                ..Default::default()
            })
        })
        .collect();

    Missiles { players, squad: squad_totals }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawEvent, RawHeader, RawLog};
    use crate::model::{Encounter, Player};

    fn raw_from(events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![],
            skills: vec![],
            events,
            guid_map: vec![],
        }
    }

    fn missile_ev(time: u64, statechange: u8, src: u64, dst: u64, is_flanking: u8, pad: u32) -> RawEvent {
        RawEvent {
            time,
            src_agent: src,
            dst_agent: dst,
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
            is_ninety: 0, is_moving: 0,
            is_statechange: statechange,
            is_flanking,
            is_shields: 0,
            is_offcycle: 0,
            pad,
        }
    }

    fn player(addr: u64) -> Player {
        Player {
            agent_addr: addr,
            account: ":A.1".into(),
            character: "Alice".into(),
            profession: "Thief".into(),
            elite_spec: "".into(),
            team: "red".into(),
            subgroup: 1,
            in_squad: true,
            commander: false,
            marker: None,
            commander_tag: None,
            agent_addrs: vec![addr],
        }
    }

    fn empty_enc() -> Encounter {
        Encounter {
            kind: "wvw".into(),
            map: "".into(),
            duration_ms: 0,
            build: "".into(),
            revision: 1,
            recorded_by: None,
            teams: vec![],
            players: vec![],
            enemies: vec![],
            markers: vec![],
            tick_rate: None,
        }
    }

    /// `fired` counts one per `CBTS_MISSILECREATE` owned by the player,
    /// regardless of what (if anything) happens afterward.
    #[test]
    fn fired_counts_missile_create_events() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
            missile_ev(10, sc::MISSILE_CREATE, 1, 0, 0, 101),
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players[0].fired, 2);
        assert_eq!(m.players[0].hit, 0);
        assert_eq!(m.players[0].denied, 0);
    }

    /// Wrong-mapping probe: `is_flanking != 0` on `CBTS_MISSILEREMOVE` MUST
    /// map to `hit`, not `denied` -- a decoder that inverted the boolean
    /// (or read the wrong byte) fails this assertion.
    #[test]
    fn remove_with_flanking_set_is_a_hit() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
            missile_ev(50, sc::MISSILE_REMOVE, 1, 0, 1, 100), // is_flanking=1 -> DidHit
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players[0].fired, 1);
        assert_eq!(m.players[0].hit, 1, "is_src_flanking != 0 on MISSILEREMOVE must map to hit");
        assert_eq!(m.players[0].denied, 0);
    }

    /// Wrong-mapping probe: `is_flanking == 0` on `CBTS_MISSILEREMOVE` MUST
    /// map to `denied`, not `hit`.
    #[test]
    fn remove_without_flanking_is_denied() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
            missile_ev(50, sc::MISSILE_REMOVE, 1, 0, 0, 100), // is_flanking=0 -> !DidHit
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players[0].fired, 1);
        assert_eq!(m.players[0].hit, 0);
        assert_eq!(m.players[0].denied, 1, "is_src_flanking == 0 on MISSILEREMOVE must map to denied");
    }

    /// A missile created but never removed (still in flight at log end)
    /// counts toward `fired` only -- neither `hit` nor `denied` claims an
    /// outcome that never resolved.
    #[test]
    fn unresolved_missile_counts_fired_only() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100)]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players[0].fired, 1);
        assert_eq!(m.players[0].hit, 0);
        assert_eq!(m.players[0].denied, 0);
    }

    /// Correlation is by trackable id (`pad`), not event order alone: two
    /// interleaved missile instances with different trackable ids must not
    /// cross-attribute their remove outcomes.
    #[test]
    fn trackable_id_correlates_remove_to_the_right_instance() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
            missile_ev(1, sc::MISSILE_CREATE, 1, 0, 0, 200),
            missile_ev(50, sc::MISSILE_REMOVE, 1, 0, 1, 200), // hit -- id 200
            missile_ev(60, sc::MISSILE_REMOVE, 1, 0, 0, 100), // denied -- id 100
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players[0].fired, 2);
        assert_eq!(m.players[0].hit, 1);
        assert_eq!(m.players[0].denied, 1);
    }

    /// A trackable id reused by a LATER `CBTS_MISSILECREATE` starts a brand
    /// new instance -- a REMOVE after the second CREATE must attach to the
    /// second instance, not the first (id recycling, per this module's doc
    /// comment).
    #[test]
    fn reused_trackable_id_starts_a_new_instance() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
            missile_ev(10, sc::MISSILE_REMOVE, 1, 0, 1, 100), // resolves instance #1: hit
            missile_ev(20, sc::MISSILE_CREATE, 1, 0, 0, 100), // id 100 reused: instance #2
            missile_ev(30, sc::MISSILE_REMOVE, 1, 0, 0, 100), // resolves instance #2: denied
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players[0].fired, 2);
        assert_eq!(m.players[0].hit, 1);
        assert_eq!(m.players[0].denied, 1);
    }

    /// `reflected_at_self`: a relaunch (`is_flanking == 0`, i.e. NOT first
    /// launch) whose target is the missile's own owner sets the heuristic;
    /// the FIRST launch (`is_flanking != 0`) targeting the owner must NOT
    /// (a missile can legitimately be launched back toward its own creator
    /// as a first launch, e.g. a boomerang-style skill -- only a relaunch
    /// distinguishes an actual bounce).
    #[test]
    fn reflected_at_self_requires_a_non_first_relaunch_at_the_owner() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
            missile_ev(1, sc::MISSILE_LAUNCH, 1, 9, 1, 100), // first launch, at enemy 9 -- not reflected
            missile_ev(2, sc::MISSILE_LAUNCH, 1, 1, 0, 100), // relaunch, target == owner (1) -- reflected
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players[0].reflected_at_self, 1);
    }

    #[test]
    fn first_launch_at_owner_is_not_counted_as_reflected() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
            missile_ev(1, sc::MISSILE_LAUNCH, 1, 1, 1, 100), // first launch, target == owner
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players[0].reflected_at_self, 0);
    }

    /// Squad-level `incoming_fired`/`incoming_denied`: an enemy-owned
    /// missile (owner not in `squad`) whose launch targets a squad member
    /// counts as incoming; if it resolves without hitting, it additionally
    /// counts as incoming-denied. No per-player credit is produced for it
    /// (per this module's doc comment).
    #[test]
    fn incoming_denied_counts_enemy_missiles_that_missed_the_squad() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 9, 0, 0, 100), // owner 9 -- not squad
            missile_ev(1, sc::MISSILE_LAUNCH, 9, 1, 1, 100), // targets squad player 1
            missile_ev(50, sc::MISSILE_REMOVE, 9, 0, 0, 100), // denied
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.squad.incoming_fired, 1);
        assert_eq!(m.squad.incoming_denied, 1);
        // Enemy owner isn't a squad player -- no PlayerMissiles entry
        // credit anywhere (the single roster entry, player 1, only fired
        // its own missiles, none here).
        assert_eq!(m.players[0].fired, 0);
    }

    /// Counterpart: an incoming enemy missile that DID hit a squad member
    /// must NOT count as `incoming_denied`.
    #[test]
    fn incoming_hit_does_not_count_as_denied() {
        let p = player(1);
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 9, 0, 0, 100),
            missile_ev(1, sc::MISSILE_LAUNCH, 9, 1, 1, 100),
            missile_ev(50, sc::MISSILE_REMOVE, 9, 0, 1, 100), // hit
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.squad.incoming_fired, 1);
        assert_eq!(m.squad.incoming_denied, 0);
    }

    /// Squad-on-squad missiles (friendly fire / self-targeted skills) must
    /// NOT count toward `incoming_fired` -- only a genuinely non-squad
    /// owner counts as "incoming".
    #[test]
    fn squad_owned_missile_at_a_squad_member_is_not_incoming() {
        let enc = Encounter { players: vec![player(1), player(2)], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
            missile_ev(1, sc::MISSILE_LAUNCH, 1, 2, 1, 100), // squad player 1 -> squad player 2
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.squad.incoming_fired, 0);
    }

    /// Squad totals are the sum of every squad player's own fired/hit/denied
    /// -- verified directly rather than assumed.
    #[test]
    fn squad_totals_sum_player_totals() {
        let enc = Encounter { players: vec![player(1), player(2)], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100),
            missile_ev(10, sc::MISSILE_REMOVE, 1, 0, 1, 100), // hit
            missile_ev(20, sc::MISSILE_CREATE, 2, 0, 0, 200),
            missile_ev(30, sc::MISSILE_REMOVE, 2, 0, 0, 200), // denied
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.squad.fired, 2);
        assert_eq!(m.squad.hit, 1);
        assert_eq!(m.squad.denied, 1);
    }

    /// Account folding: missile events from EITHER of an account's raw
    /// addrs (relog) land on the same single `PlayerMissiles` entry, like
    /// every other per-player metric this project computes.
    #[test]
    fn folds_missile_events_across_relog_addrs() {
        let mut p = player(1);
        p.agent_addrs = vec![1, 2];
        let enc = Encounter { players: vec![p], ..empty_enc() };
        let raw = raw_from(vec![
            missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100), // pre-relog addr
            missile_ev(10, sc::MISSILE_REMOVE, 1, 0, 1, 100),
            missile_ev(20, sc::MISSILE_CREATE, 2, 0, 0, 200), // post-relog addr, same account
            missile_ev(30, sc::MISSILE_REMOVE, 2, 0, 0, 200),
        ]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players.len(), 1);
        assert_eq!(m.players[0].fired, 2);
        assert_eq!(m.players[0].hit, 1);
        assert_eq!(m.players[0].denied, 1);
    }

    /// Every roster player gets an entry even with zero missile activity --
    /// mirrors `analysis::replay::Track` always having one entry per
    /// roster member.
    #[test]
    fn every_squad_player_gets_an_entry_even_with_no_missiles() {
        let enc = Encounter { players: vec![player(1), player(2)], ..empty_enc() };
        let raw = raw_from(vec![missile_ev(0, sc::MISSILE_CREATE, 1, 0, 0, 100)]);
        let m = build_missiles(&raw, &enc);
        assert_eq!(m.players.len(), 2);
        assert_eq!(m.players[1].agent_addr, 2);
        assert_eq!(m.players[1].fired, 0);
    }
}
