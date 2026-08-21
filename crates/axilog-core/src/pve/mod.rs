//! Encounter identity for non-WvW logs: which fight is this, and was it won.
//!
//! arcdps tells a parser exactly one thing about which encounter it
//! recorded: the *trigger species id*, in bytes 13-14 of the evtc header
//! ([`crate::evtc::RawHeader::boss_id`]). Everything else -- the fight's
//! name, whether it was a raid or a fractal, whether the squad killed the
//! boss -- has to be derived.
//!
//! Until this module existed, nothing derived it. [`crate::model::resolve`]
//! hardcoded `kind: "wvw"` and `map: "World vs World"` for every log, and
//! [`axilog_ei`] rendered `fightName` as `"Detailed WvW - {map}"`
//! unconditionally, so a Gorseval kill and a Samarog kill both came out of
//! `--format ei-json` as `"Detailed WvW - World vs World"`. That is the bug
//! this module fixes; axibridge showed those strings verbatim in its fight
//! list.
//!
//! # Naming, and why the table is small
//!
//! GW2EI's default fight name (`LogLogic.GetLogicName`) is *the character
//! name of the target whose species is the trigger id* -- the boss's own
//! agent name, already sitting in the log's agent table. So the general
//! rule needs no table at all: read the header id, find the agent, use its
//! name. That is [`identify`]'s main path, and it names encounters GW2EI
//! has never heard of as readily as ones it has.
//!
//! [`encounters::ENCOUNTERS`] is the correction layer on top: the encounter
//! *category* (which nothing in the log states) for the 90 ids GW2EI knows,
//! plus a fixed name for the ~38 whose name is not any single agent's --
//! "Twin Largos" is Nikare and Kenut, "Harvest Temple" is a dozen
//! dragonvoid gadgets, "Siege the Stronghold" is an escort event with no
//! boss at all.
//!
//! # What this module does NOT do
//!
//! **Challenge Mote / Legendary CM.** GW2EI decides those per encounter,
//! with 45 bespoke detectors keyed on boss health pools, specific skill
//! casts and game-build gates. None of that is transcribed here, so a
//! fractal CM is named `"Skorvald"`, not `"Skorvald CM"`. A few encounters
//! get it free anyway, because ArenaNet gave the challenge version its own
//! species id and GW2EI's table already separates them (`MinisterLiCM`,
//! `DecimaCM`, the Old Lion's Court prototypes).
//!
//! **Per-encounter success rules.** [`succeeded`] implements only GW2EI's
//! *generic* fallback (`LogLogic.NoBouncyChestGenericCheckSucess` ->
//! `SetSuccessByDeath`): the fight was won if every agent of the trigger
//! species died. Encounters GW2EI succeeds by reward-chest gadget, by
//! combat exit, or by a scripted event -- escorts, Twisted Castle, the
//! Statues -- are not covered, and will read as a failure even when they
//! were won. See that function's doc for the precise rule.
//!
//! **Target selection.** `enc.enemies` still holds every NPC in the log,
//! un-ranked; nothing here promotes the boss to a "target" the way EI's
//! per-logic `Targets` list does. `axilog_schema`'s `ei_targets` remains
//! gated on `kind == "wvw"` and stays empty for PvE, exactly as before this
//! module -- naming a fight and analysing it are separate jobs.

pub mod encounters;

use crate::evtc::{sc, RawAgent, RawLog};
use crate::model::{agent_kind, AgentKind};

/// arcdps's WvW trigger id (GW2EI `TargetID.WorldVersusWorld`). A log
/// carrying this id -- or no id at all -- is not a PvE encounter and is
/// left to [`crate::wvw`], which names it after the map.
pub const WVW_TRIGGER_ID: u32 = 1;

/// What [`identify`] could work out about the encounter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The evtc header's trigger species id, verbatim.
    pub trigger_id: u32,
    /// GW2EI's `LogCategory` slug -- `"raid_wing"`, `"fractal"`,
    /// `"raid_encounter"`, `"golem"`, `"story"`, `"open_world"`,
    /// `"convergence"`, `"unknown_encounter"` -- or `"unknown"` for a
    /// trigger id GW2EI has no logic for. Becomes
    /// [`crate::model::Encounter::kind`].
    pub kind: &'static str,
    /// The fight's display name.
    pub name: String,
    /// GW2EI's `SubLogCategory` (`"SpiritVale"`, `"ShatteredObservatory"`),
    /// when the catalog declares one -- the wing/fractal grouping.
    pub sub_category: Option<&'static str>,
    /// Whether the trigger species is in this project's transcription of
    /// GW2EI's table. `false` means the name came from the agent table
    /// alone and [`Self::kind`] is `"unknown"` -- a new boss, or a log
    /// GW2EI does not support either.
    pub catalogued: bool,
    /// Whether the squad won, by the generic rule [`succeeded`] documents.
    /// Trustworthy when `true`, only suggestive when `false`.
    pub success: bool,
    /// Agent addresses of the trigger species -- the boss (or bosses) the
    /// encounter is named for, in agent-table order.
    ///
    /// This is the PvE answer to "which enemies is this fight ABOUT", the
    /// question GW2EI's per-logic `Targets` list answers and which WvW
    /// answers with "every enemy player". `axilog_schema` uses it to pick
    /// `targets[]` for a PvE log, so that a raid reports its boss instead
    /// of all 265 ambient NPCs in the instance.
    ///
    /// It is deliberately NOT the full EI target list: EI promotes split
    /// phases, adds friendly NPCs and names sub-targets per encounter.
    /// This is the trigger species and nothing else.
    pub target_addrs: Vec<u64>,
}

/// Every non-player agent of species `trigger_id`.
///
/// NPCs are preferred over gadgets, and gadgets only considered when no
/// NPC matches. Species ids are not disjoint across the two -- arcdps
/// packs a gadget's id into the same low 16 bits of `prof` that carry an
/// NPC's species -- and a handful of trigger ids GW2EI lists ARE gadgets
/// (`EtherealBarrierGadget`, the dragonvoid pair). Preferring the NPC
/// keeps a coincidental gadget collision from outvoting the real boss;
/// falling back to gadgets keeps those gadget-triggered encounters
/// working.
fn trigger_agents(raw: &RawLog, trigger_id: u32) -> Vec<&RawAgent> {
    let matches = |a: &&RawAgent, want: AgentKind| {
        agent_kind(a) == want && (a.prof & 0xffff) == trigger_id
    };
    let npcs: Vec<&RawAgent> =
        raw.agents.iter().filter(|a| matches(a, AgentKind::Npc)).collect();
    if !npcs.is_empty() {
        return npcs;
    }
    raw.agents.iter().filter(|a| matches(a, AgentKind::Gadget)).collect()
}

/// Identify the encounter a raw log recorded, or `None` for a WvW log.
///
/// `None` is returned for trigger id 0 (a log with no encounter, e.g. a
/// synthetic or truncated capture) and for [`WVW_TRIGGER_ID`], because
/// both belong to [`crate::wvw`]'s map-name path rather than this one.
/// Callers should keep their existing WvW behaviour on `None` rather than
/// substituting a PvE default -- that is the whole reason this returns an
/// `Option` instead of an `Identity` with `kind: "wvw"`.
pub fn identify(raw: &RawLog) -> Option<Identity> {
    let trigger_id = raw.header.boss_id as u32;
    if trigger_id == 0 || trigger_id == WVW_TRIGGER_ID {
        return None;
    }
    let def = encounters::lookup(trigger_id);

    // GW2EI's fixed name first (it exists precisely for the fights no one
    // agent names), then GW2EI's DEFAULT rule -- the trigger agent's own
    // name -- then a last resort that still carries the id, so an
    // unidentifiable log is debuggable rather than blank.
    let name = def
        .and_then(|d| d.name)
        .map(str::to_string)
        .or_else(|| {
            trigger_agents(raw, trigger_id)
                .first()
                .map(|a| a.name_parts().0)
                .filter(|n| !n.is_empty())
        })
        .unwrap_or_else(|| format!("Unknown Encounter {trigger_id}"));

    let bosses = trigger_agents(raw, trigger_id);
    Some(Identity {
        trigger_id,
        kind: def.map(|d| d.category).unwrap_or("unknown"),
        name,
        sub_category: def.and_then(|d| d.sub_category),
        catalogued: def.is_some(),
        success: succeeded(raw, trigger_id),
        target_addrs: bosses.iter().map(|a| a.addr).collect(),
    })
}

/// Whether the squad won, by GW2EI's *generic* success rule only.
///
/// GW2EI's `LogLogic.NoBouncyChestGenericCheckSucess` tries three things in
/// order -- the encounter's reward-chest gadget, then death of every
/// success-check target, then those targets leaving combat -- and each
/// encounter may override the set of targets, add its own scripted check,
/// or replace the whole thing. Only the middle rule is implemented here,
/// over the trigger species rather than a per-logic target list:
///
/// > the fight was won if there is at least one agent of the trigger
/// > species and every one of them has a `CBTS_CHANGEDEAD` event.
///
/// The consequences are asymmetric, and worth stating plainly: a `true`
/// here is reliable (the boss is dead), a `false` is not (the boss may
/// have been defeated by a mechanic that never kills its agent). Encounters
/// won without killing the trigger agent -- Siege the Stronghold, Twisted
/// Castle, the River of Souls, the Hall of Chains statues, anything GW2EI
/// succeeds by chest -- therefore report `false` on a successful run.
///
/// WvW logs never reach this: GW2EI's `WvWLogic` has no failure state and
/// reports success unconditionally, which is what callers should keep
/// doing for `kind == "wvw"`.
pub fn succeeded(raw: &RawLog, trigger_id: u32) -> bool {
    let bosses = trigger_agents(raw, trigger_id);
    if bosses.is_empty() {
        return false;
    }
    bosses.iter().all(|boss| {
        raw.events
            .iter()
            .any(|e| e.is_statechange == sc::CHANGE_DEAD && e.src_agent == boss.addr)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evtc::{RawAgent, RawEvent, RawHeader, RawLog};

    fn npc(addr: u64, species: u32, name: &str) -> RawAgent {
        RawAgent {
            addr,
            prof: species,
            is_elite: 0xffff_ffff,
            toughness: 0,
            concentration: 0,
            healing: 0,
            hitbox_width: 0,
            condition: 0,
            hitbox_height: 0,
            name_raw: name.as_bytes().to_vec(),
        }
    }

    fn gadget(addr: u64, species: u32, name: &str) -> RawAgent {
        RawAgent { prof: 0xffff_0000 | species, ..npc(addr, species, name) }
    }

    fn death(addr: u64) -> RawEvent {
        RawEvent {
            time: 0, src_agent: addr, dst_agent: 0, value: 0, buff_dmg: 0,
            overstack: 0, skillid: 0, src_instid: 0, dst_instid: 0,
            src_master_instid: 0, dst_master_instid: 0, iff: 0, buff: 0,
            result: 0, is_activation: 0, is_buffremove: 0, is_ninety: 0,
            is_fifty: 0, is_moving: 0, is_statechange: sc::CHANGE_DEAD,
            is_flanking: 0, is_shields: 0, is_offcycle: 0, pad: 0,
        }
    }

    fn log(boss_id: u16, agents: Vec<RawAgent>, events: Vec<RawEvent>) -> RawLog {
        RawLog {
            header: RawHeader { build: "20260811".into(), revision: 1, boss_id },
            agents,
            skills: Vec::new(),
            events,
            guid_map: Vec::new(),
        }
    }

    #[test]
    fn wvw_and_empty_trigger_ids_are_not_pve_encounters() {
        // The two ids that belong to `crate::wvw`, not here. Returning an
        // `Identity` for either -- even one saying "wvw" -- would invite a
        // caller to overwrite the map name that path spent real work
        // resolving.
        for id in [0u16, WVW_TRIGGER_ID as u16] {
            assert_eq!(identify(&log(id, vec![], vec![])), None, "trigger id {id}");
        }
    }

    #[test]
    fn a_catalogued_boss_is_named_after_its_own_agent() {
        // GW2EI's DEFAULT rule, which covers most of the table: the name is
        // the trigger agent's, NOT anything the catalog stores. The catalog
        // supplies only the category.
        let raw = log(15429, vec![npc(1, 15429, "Gorseval the Multifarious")], vec![]);
        let id = identify(&raw).unwrap();
        assert_eq!(id.name, "Gorseval the Multifarious");
        assert_eq!(id.kind, "raid_wing");
        assert_eq!(id.sub_category, Some("SpiritVale"));
        assert!(id.catalogued);
    }

    #[test]
    fn a_fixed_name_beats_the_agent_name() {
        // Nikare's agent is called "Nikare"; the FIGHT is called "Twin
        // Largos", because Kenut is in it too. The catalog exists for this
        // case, so it has to win over the agent-table default.
        let raw = log(21105, vec![npc(1, 21105, "Nikare")], vec![]);
        assert_eq!(identify(&raw).unwrap().name, "Twin Largos");
    }

    #[test]
    fn an_uncatalogued_boss_is_still_named_from_the_agent_table() {
        // A boss GW2EI has no logic for -- next expansion's, or one it
        // never supported. The category is honestly `"unknown"`, but the
        // name is real, which is the whole advantage of deriving it from
        // the log instead of a table.
        let raw = log(64000, vec![npc(1, 64000, "Some New Boss")], vec![]);
        let id = identify(&raw).unwrap();
        assert_eq!(id.name, "Some New Boss");
        assert_eq!(id.kind, "unknown");
        assert!(!id.catalogued);
    }

    #[test]
    fn an_unresolvable_encounter_keeps_the_id_in_the_name() {
        // No catalog entry and no agent of that species. The name must
        // still say WHICH id could not be resolved: a bare "Unknown
        // Encounter" in a fight list is unactionable, and an empty string
        // reads as a rendering bug rather than a parsing gap.
        let raw = log(64000, vec![npc(1, 15429, "Gorseval the Multifarious")], vec![]);
        assert_eq!(identify(&raw).unwrap().name, "Unknown Encounter 64000");
    }

    #[test]
    fn npc_agents_outrank_gadgets_of_the_same_species_id() {
        // arcdps packs gadget ids into the same low 16 bits of `prof` that
        // carry NPC species, so the two namespaces collide. The NPC is the
        // boss; the gadget sharing its number is a coincidence.
        let raw = log(
            15429,
            vec![
                gadget(1, 15429, "Some Gadget"),
                npc(2, 15429, "Gorseval the Multifarious"),
            ],
            vec![],
        );
        assert_eq!(identify(&raw).unwrap().name, "Gorseval the Multifarious");
    }

    #[test]
    fn a_gadget_triggered_encounter_still_resolves() {
        // ...but when NOTHING is an NPC of that species, gadgets are all
        // there is. Three of GW2EI's own trigger ids are gadgets, so the
        // fallback is load-bearing, not defensive.
        let raw = log(47188, vec![gadget(1, 47188, "Ethereal Barrier")], vec![]);
        // The catalog's fixed name wins here, as it does for every
        // multi-agent encounter; what is being asserted is that the gadget
        // was found at all, i.e. the category resolved.
        let id = identify(&raw).unwrap();
        assert_eq!(id.name, "Spirit Race");
        assert_eq!(id.kind, "raid_wing");
    }

    #[test]
    fn success_needs_every_trigger_agent_dead() {
        let bosses = vec![npc(1, 21105, "Nikare"), npc(2, 21105, "Nikare")];
        assert!(!super::succeeded(&log(21105, bosses.clone(), vec![]), 21105));
        assert!(!super::succeeded(&log(21105, bosses.clone(), vec![death(1)]), 21105));
        assert!(super::succeeded(&log(21105, bosses, vec![death(1), death(2)]), 21105));
    }

    #[test]
    fn a_log_with_no_trigger_agent_is_not_a_success() {
        // Vacuous truth is the danger here: "all zero bosses are dead" is
        // `true` for `Iterator::all`, and would report a win for a log
        // that never contained the boss.
        assert!(!super::succeeded(&log(15429, vec![], vec![]), 15429));
    }
}
