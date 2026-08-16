//! Best-effort `skillMap` built from the log's OWN skill table -- M14, Task 2.
//!
//! # Scope, honestly
//!
//! Real GW2EI's `skillMap` is built from a combination of the log's own
//! embedded skill-name table AND GW2EI's own bundled skill database (backed
//! by the live GW2 API, `https://api.guildwars2.com/v2/skills`) -- the API
//! copy supplies richer/disambiguated names (e.g. `"Flame Blast (Minor/
//! Major/Superior Sigil of Fire)"` for skill id 9284, vs whatever shorter
//! string arcdps itself wrote into the log's skill table for that id, if
//! any), a render.guildwars2.com icon URL, and several per-skill classifier
//! flags (`isInstantCast`, `isTraitProc`, `isUnconditionalProc`,
//! `isGearProc`, `isNotAccurate`, `conversionBasedHealing`, `hybridHealing`
//! -- see a real dps.report export's `skillMap[*]` shape, spot-checked
//! against `fixtures/local/wvw-postrework.ei.json` by this module's own
//! golden test). **This module deliberately does NOT attempt any of that.**
//!
//! One correction to the sentence above, established by a 2026-08-16 spike
//! (recorded in `docs/ROADMAP.md` under MPROC): the proc/instant family
//! (`isTraitProc`/`isGearProc`/`isUnconditionalProc`/`isNotAccurate`) does
//! NOT need the external database. Those four are a side effect of GW2EI's
//! instant-cast detection subsystem -- `CombatData.cs:214-244` fills the sets
//! from each `InstantCastFinder`'s declared `CastOrigin`, and `SkillData.cs`
//! only does `Contains`. They are reproducible, but the cost is porting 658
//! finders, and the answer is LOG-SPECIFIC (availability is gated on GW2/evtc
//! build ranges *and* on arbitrary predicates over the parsed log), so no
//! static table can match EI. `isInstantCast` is stricter still: it asks
//! whether a finder actually FIRED in this log
//! (`GW2EIBuilders/JsonModels/JsonLogBuilder.cs:23`), so it cannot be
//! shortcut at all. Only `conversionBasedHealing`/`hybridHealing` and the
//! richer names/icon are genuinely database-backed -- and two neighbours in
//! the same struct, `canCrit` and `isSwap`, are cheap static id tests that
//! need neither the database nor the finders.
//! It builds a much smaller, purely-LOG-DERIVED map:
//!
//! - `name`: straight from the log's own `cbtskill` table
//!   (`evtc::skill::RawSkill.name`, already decoded/trimmed at container
//!   parse time), trimmed again defensively and falling back to `"Skill
//!   <id>"` for an empty or purely-numeric name (arcdps itself sometimes
//!   writes a placeholder numeric string, or nothing at all, for an id it
//!   didn't have a display name cached for at capture time). No icon, no
//!   API disambiguation, no proc/instant/accuracy classification.
//! - `can_crit`: objectively computable from the id alone (see below) --
//!   NOT part of the name gap, and matches EI exactly.
//! - `is_swap`: ALSO objectively computable from the id alone, and as of
//!   2026-08-16 a COMPLETE port of EI's own `SkillItem.IsSwap` -- see its
//!   own section below for the per-category citations and for the Weaver
//!   table that used to be this field's one documented gap.
//! - `auto_attack`: OMITTED (see its own section below) -- genuinely not
//!   derivable from this log's own data, so this module refuses to guess.
//!
//! `skill_map_golden.rs`'s spot-check test documents the resulting overlap
//! (ids present in both: `can_crit` compared EXACTLY, `is_swap` divergences
//! COUNTED and printed, not hard-failed -- see its own section below) AND
//! divergence (name strings, side-by-side, real examples) against a real
//! dps.report export -- it does NOT hard-fail on a name mismatch, since the
//! two are fundamentally different data sources, not a calibration target.
//!
//! # Scope of referenced ids: only what squad players actually touch
//!
//! Per the M14 plan brief ("only skills REFERENCED by squad players
//! (damage, rotation, buffs)"), this is NOT a dump of the log's full
//! `RawLog::skills` table (a real WvW log's skill table commonly has ~1,000
//! entries, the vast majority irrelevant to anything this project actually
//! reports -- e.g. NPC-only skills, environmental effects). The referenced
//! set is the union of:
//! - every skill id in any squad player's `PlayerMetrics::skill_damage`
//!   (`outgoing`/`taken`/`per_target[].skills`, M12 Task 1),
//! - every skill id in any squad player's `PlayerMetrics::rotation` (M14
//!   Task 1),
//! - the 12 tracked-boon ids (`buffs::BOON_IDS`) -- always included, since
//!   every player's native `boons[]` block always names all 12 by id,
//!   regardless of whether this particular fight happened to show one.
//!
//! An id can appear in this referenced set without ever having a matching
//! `RawLog::skills` entry at all (e.g. a synthetic/internal id, or a real
//! skill the log's own table happened not to include a name row for) --
//! `name` still resolves via the same empty-name fallback in that case,
//! `"Skill <id>"`.
//!
//! # Measured size: always-on, not opt-in
//!
//! Unlike `skill_damage`/`timeseries`/`rotation` above (each gated behind
//! an opt-in flag after measuring >30% JSON growth), `Metrics::skill_map`
//! is always computed AND always serialized -- no gating flag. Measured on
//! the committed fixture (`fixtures/wvw-small.anon.zevtc`, every other
//! opt-in block off): 368 referenced skill ids, a 24,309-byte JSON block,
//! growing the rendered HTML report from 236,198 to 260,520 bytes
//! (**+10.3%**) -- real, but well under the ~30% guideline the OTHER
//! blocks were measured against before being gated, since this map is
//! scoped to only referenced ids (see above), not the whole ~1,000-entry
//! log skill table. `axilog-html/tests/golden_html.rs`'s
//! `total_report_size_stays_under_budget` gate was raised from 250,000 to
//! 275,000 bytes to absorb this (see that test's own doc comment for the
//! same numbers) -- a budget adjustment, not a new opt-in flag, per this
//! milestone's own plan brief ("don't gate a small map").
//!
//! # `is_swap`: the weapon-swap pseudo id
//!
//! arcdps has NO real skill id for "the player swapped weapons" -- the wire
//! event is a dedicated statechange, `CBTS_WEAPSWAP` (`is_statechange ==
//! 11`), carrying `src_agent`=the swapping agent, `dst_agent`=new weapon
//! set id, `value`=old weapon set id (verified directly against the live
//! arcdps EVTC reference, `https://www.deltaconnected.com/arcdps/evtc/
//! README.txt`, 2026-08-09: `CBTS_WEAPSWAP, // agent weapon set changed` /
//! `// src_agent: relates to agent` / `// dst_agent: new weapon set id` /
//! `// value: old weapon seet id`) -- no `skillid` field is meaningfully
//! populated on that row at all. GW2EI invents a PSEUDO skill id for this
//! case purely so its own `rotation[]`/`skillMap` can represent a weapon
//! swap as just another cast entry: `SkillIDs.WeaponSwap = -2` (verified
//! against `baaron4/GW2-Elite-Insights-Parser`, `master`,
//! `GW2EIEvtcParser/ParserHelpers/IDs/SkillIDs.cs`, 2026-08-09 -- the same
//! constant this project's own `analysis::rotation` module doc already
//! cites for its own, separate, documented `WeaponSwapEvent` scope gap).
//! [`WEAPON_SWAP_SKILL_ID`] reproduces that same sentinel (`-2i32 as u32`,
//! since this project's skill ids are unsigned throughout).
//!
//! On this module's OWN referenced scope, the literal `-2` sentinel can
//! never actually fire: `-2` is not a real game skill id (never appears in
//! `RawLog::skills`, never appears in `skill_damage` since it's not a
//! damage event), and `analysis::rotation` deliberately does NOT decode
//! `CBTS_WEAPSWAP`/`WeaponSwapEvent` casts at all (see that module's own
//! "Deliberately OUT OF SCOPE" doc section) -- so it never contributes a
//! rotation-group skill id either. `is_swap` is still implemented (and
//! unit-tested against the sentinel directly) because it's a cheap,
//! objectively-correct-by-construction check, and matches EI's own
//! `isSwap` field shape for any FUTURE caller that widens the referenced
//! scope (e.g. if a later milestone decodes `WeaponSwapEvent` casts too).
//!
//! ## Extended non-sentinel `is_swap` ids (M14, Task 3, Ruling B)
//!
//! Discovered empirically by this module's own golden spot-check
//! (`skill_map_golden.rs`, against `fixtures/local/wvw-postrework.ei.json`,
//! a real dps.report export) and then confirmed directly in GW2EI source
//! (`baaron4/GW2-Elite-Insights-Parser`, `master`, `GW2EIEvtcParser/
//! ParsedData/Skills/SkillItem.cs`, 2026-08-09): real EI's `IsSwap` is
//! ```text
//! public bool IsSwap => ID == WeaponSwap
//!     || ElementalistHelper.IsAttunementSwap(ID)
//!     || WeaverHelper.IsAttunementSwap(ID)
//!     || RevenantHelper.IsLegendSwap(ID)
//!     || HeraldHelper.IsLegendSwap(ID)
//!     || RenegadeHelper.IsLegendSwap(ID)
//!     || VindicatorHelper.IsLegendSwap(ID)
//!     || ConduitHelper.IsLegendSwap(ID)
//!     || NecromancerHelper.IsDeathShroudTransform(ID)
//!     || HarbingerHelper.IsHarbingerShroudTransform(ID)
//!     || RitualistHelper.IsRitualistShroudTransform(ID);
//! ```
//! -- i.e. "swap" covers not just the literal weapon-swap pseudo id but
//! EVERY profession's own "change stance" mechanic. Per the M14 Task 3 plan
//! brief (Ruling B of the review), this module now ALSO reproduces the
//! three NAMED, curated categories the brief called out -- each verified
//! directly against the same `/tmp/gw2ei` checkout of `baaron4/
//! GW2-Elite-Insights-Parser`, `master`, 2026-08-09:
//!
//! - **Elementalist attunement swaps** (`ElementalistHelper.
//!   IsAttunementSwap`, `GW2EIEvtcParser/EIData/ProfHelpers/Elementalist/
//!   ElementalistHelper.cs`): the base 4 core-Elementalist attunement-swap
//!   skill ids, from `GW2EIEvtcParser/ParserHelpers/IDs/SkillIDs.cs`:
//!   `FireAttunementSkill=5492`, `WaterAttunementSkill=5493`,
//!   `AirAttunementSkill=5494`, `EarthAttunementSkill=5495` -- id `5492` is
//!   ALSO one of `hit_stats::NON_CRITABLE_SKILLS`'s 20 entries, a real,
//!   confirmed `is_swap`/`can_crit` divergence on the same id (both flags
//!   are independently correct: `5492` is both a swap AND non-critable).
//! - **Revenant legend swaps, 5 variants** (`RevenantHelper.IsLegendSwap`
//!   plus each elite spec's own override, `GW2EIEvtcParser/EIData/
//!   ProfHelpers/Revenant/{RevenantHelper,HeraldHelper,RenegadeHelper,
//!   VindicatorHelper,ConduitHelper}.cs`): base Revenant's 4
//!   `LegendaryAssassinStanceSkill=28134`, `LegendaryDemonStanceSkill=
//!   28494`, `LegendaryDwarfStanceSkill=28419`,
//!   `LegendaryCentaurStanceSkill=28195`; Herald's `LegendaryDragonStanceSkill
//!   =28085`; Renegade's `LegendaryRenegadeStanceSkill=41858`; Vindicator's
//!   `LegendaryAllianceStanceSkill=62749`; Conduit's
//!   `LegendaryEntityStanceSkill=76610` (all 8 ids from the same
//!   `SkillIDs.cs`).
//! - **Necromancer shroud transforms, 3 variants**
//!   (`NecromancerHelper.IsDeathShroudTransform`/`HarbingerHelper.
//!   IsHarbingerShroudTransform`/`RitualistHelper.
//!   IsRitualistShroudTransform`, same `ProfHelpers/Necromancer/` dir --
//!   deliberately NOT `ReaperHelper.IsReaperShroudTransform`, which reuses
//!   base Death Shroud's own enter/exit ids rather than defining its own,
//!   matching real EI's `IsSwap` itself never calling that 4th helper):
//!   Necromancer's `EnterDeathShroud=10574`/`ExitDeathShroud=10585`;
//!   Harbinger's `EnterHarbingerShroud=62567`/`ExitHarbingerShroud=62540`;
//!   Ritualist's `EnterRitualistsShroud=77238`/`ExitRitualistsShroud=76933`
//!   (all 6 ids from the same `SkillIDs.cs`).
//!
//! ### Weaver's dual-attunement table: the last `is_swap` gap, now closed
//!
//! M14 Task 3 deliberately stopped at the 3 categories above, leaving
//! `WeaverHelper.IsAttunementSwap` (`GW2EIEvtcParser/EIData/ProfHelpers/
//! Elementalist/WeaverHelper.cs:15-23`, `_weaverAttunements`) as a
//! documented, narrower-than-real-EI gap, on the grounds that it was a
//! "much larger 16-entry table". Re-checked 2026-08-16 against the same
//! checkout: it IS 16 entries, but **12 of them are EI-invented negative
//! pseudo ids**, exactly like the `WeaponSwap = -2` sentinel this module
//! already reproduces -- `FireWaterAttunement = -5` through
//! `EarthAirAttunement = -16` (`GW2EIEvtcParser/ParserHelpers/IDs/
//! SkillIDs.cs:24-35`). Only the 4 same-element "dual" entries are real
//! game ids: `DualWaterAttunement = 41166`, `DualAirAttunement = 42264`,
//! `DualFireAttunement = 43470`, `DualEarthAttunement = 44857` (same file,
//! lines 2627/2680/2740/2807). So the table is 16 ids but 4 facts, and the
//! stated reason for skipping it (size) did not survive contact with the
//! source. [`WEAVER_ATTUNEMENT_SWAP_SKILL_IDS`] now reproduces all 16, and
//! [`is_swap`] is a COMPLETE port of EI's own `SkillItem.IsSwap` -- every
//! one of its 11 disjuncts, with no remaining documented exclusion.
//!
//! As with the `-2` sentinel, the 12 negative ids can never fire on this
//! module's own referenced scope (this project never synthesizes Weaver's
//! cross-element pseudo ids), and the 4 real ones are attunement BUFF ids
//! -- reachable only if a future milestone widens the referenced set past
//! damage/rotation/boons. They are implemented for the same reason: cheap,
//! objectively correct by construction, and shaped for a future caller.
//! `skill_map_golden.rs`'s local spot-check still documents (does NOT
//! hard-fail on) any residual `is_swap` divergence against a real capture,
//! the same "measure and document, don't silently under-cite" discipline
//! `analysis::rotation`'s own `InstantCastEvent` gap already established
//! for this milestone.
//!
//! # `can_crit`: reused verbatim from M13
//!
//! Delegates to `hit_stats::can_crit` (M13 Task 1's `NonCritableSkills`
//! table, `GW2EIEvtcParser/ParsedData/Skills/SkillItemOverrides.cs`) --
//! same citation, same 20-id table, already calibrated exact against the
//! golden fixture for `hit_stats::HitStats::critable_direct_count`. No new
//! verification needed: `can_crit` is a pure function of the id, and this
//! module calls the exact same one.
//!
//! # `auto_attack`: OMITTED, not guessed
//!
//! Real GW2EI's own `SkillItem.IsAutoAttack` (verified against
//! `baaron4/GW2-Elite-Insights-Parser`, `master`,
//! `GW2EIEvtcParser/ParsedData/Skills/SkillItem.cs`, 2026-08-09):
//! ```text
//! public bool IsAutoAttack(ParsedEvtcLog log) => AA
//!     || GuardianHelper.IsAutoAttack(log, ID)
//!     || FirebrandHelper.IsAutoAttack(log, ID)
//!     || RevenantHelper.IsAutoAttack(log, ID)
//!     || BladeswornHelper.IsAutoAttack(log, ID);
//! ```
//! where the base `AA` flag itself is set from the GW2 **live API**'s own
//! skill metadata at construction time, NOT from anything in the arcdps
//! log:
//! ```text
//! AA = (ApiSkill?.Slot == "Weapon_1" || ApiSkill?.Slot == "Downed_1")
//!     && !ApiSkill.Categories.Contains("StealthAttack")
//!     && !ApiSkill.Description.Contains("Ambush.");
//! ```
//! (`ApiSkill` = a cached `https://api.guildwars2.com/v2/skills` response;
//! `Slot == "Weapon_1"` is the API's own "this occupies the auto-attack
//! weapon slot" flag.) On top of that base flag, four more profession-
//! specific helper classes each apply their OWN additional hardcoded
//! per-skill-id special-casing (Guardian/Firebrand/Revenant/Bladesworn
//! mechanics where the "auto attack" isn't simply the Weapon_1-slot skill).
//! None of this -- weapon-slot metadata, category tags, or the four
//! profession-specific override tables -- exists anywhere in
//! `RawLog::skills` (id + name only) or anywhere else this project decodes
//! from the EVTC wire format. Per the M14 plan brief ("if not cleanly
//! derivable from the log, OMIT with a documented note rather than
//! guess"), [`SkillMapEntry::auto_attack`] is `Option<bool>`, always `None`
//! for every entry this module builds -- the native/ei-json schema layers
//! both omit the key entirely (`#[serde(skip_serializing_if =
//! "Option::is_none")]`) rather than emit a fabricated `false`, which would
//! read as "confirmed not an auto-attack" instead of "unknown". The field
//! is kept in the type (rather than deleted) so a future data source (e.g.
//! a bundled/fetched GW2 API skill cache) can populate it without another
//! schema-shape change.

use super::hit_stats;
use super::PlayerMetrics;
use crate::evtc::RawLog;
use std::collections::{BTreeMap, BTreeSet};

/// GW2EI's `SkillIDs.WeaponSwap` pseudo skill id (`-2`, reinterpreted as
/// `u32` since this project's skill ids are unsigned throughout) -- see this
/// module's doc comment for the full citation and the "can never fire on
/// this module's own referenced scope, but implemented anyway" writeup.
pub const WEAPON_SWAP_SKILL_ID: u32 = (-2i32) as u32;

/// Elementalist attunement-swap skill ids (`ElementalistHelper.
/// IsAttunementSwap`'s base-4, distinct from Weaver's own
/// [`WEAVER_ATTUNEMENT_SWAP_SKILL_IDS`] table) -- see this module's doc
/// comment's "Extended non-sentinel `is_swap` ids" section for the full
/// citation.
const ATTUNEMENT_SWAP_SKILL_IDS: [u32; 4] = [
    5492, // FireAttunementSkill
    5493, // WaterAttunementSkill
    5494, // AirAttunementSkill
    5495, // EarthAttunementSkill
];

/// Revenant legend-swap skill ids, all 5 variants (base Revenant + Herald +
/// Renegade + Vindicator + Conduit) -- see this module's doc comment's
/// "Extended non-sentinel `is_swap` ids" section for the full citation.
const LEGEND_SWAP_SKILL_IDS: [u32; 8] = [
    28134, // LegendaryAssassinStanceSkill (base Revenant)
    28494, // LegendaryDemonStanceSkill (base Revenant)
    28419, // LegendaryDwarfStanceSkill (base Revenant)
    28195, // LegendaryCentaurStanceSkill (base Revenant)
    28085, // LegendaryDragonStanceSkill (Herald)
    41858, // LegendaryRenegadeStanceSkill (Renegade)
    62749, // LegendaryAllianceStanceSkill (Vindicator)
    76610, // LegendaryEntityStanceSkill (Conduit)
];

/// Necromancer shroud-transform skill ids, all 3 variants (base
/// Necromancer/Harbinger/Ritualist -- deliberately NOT Reaper, which reuses
/// base Death Shroud's own ids) -- see this module's doc comment's
/// "Extended non-sentinel `is_swap` ids" section for the full citation.
const SHROUD_TRANSFORM_SKILL_IDS: [u32; 6] = [
    10574, // EnterDeathShroud (Necromancer)
    10585, // ExitDeathShroud (Necromancer)
    62567, // EnterHarbingerShroud (Harbinger)
    62540, // ExitHarbingerShroud (Harbinger)
    77238, // EnterRitualistsShroud (Ritualist)
    76933, // ExitRitualistsShroud (Ritualist)
];

/// Weaver's dual-attunement ids (`WeaverHelper.IsAttunementSwap`'s
/// `_weaverAttunements`) -- 16 entries, but only the 4 same-element "dual"
/// ones are real game ids; the other 12 are EI-invented negative pseudo ids
/// (`-5`..`-16`), reinterpreted as `u32` for the same reason
/// [`WEAPON_SWAP_SKILL_ID`] is. See this module's doc comment's "Weaver's
/// dual-attunement table" section for the full citation.
const WEAVER_ATTUNEMENT_SWAP_SKILL_IDS: [u32; 16] = [
    43470,           // DualFireAttunement
    (-5i32) as u32,  // FireWaterAttunement
    (-6i32) as u32,  // FireAirAttunement
    (-7i32) as u32,  // FireEarthAttunement
    (-8i32) as u32,  // WaterFireAttunement
    41166,           // DualWaterAttunement
    (-9i32) as u32,  // WaterAirAttunement
    (-10i32) as u32, // WaterEarthAttunement
    (-11i32) as u32, // AirFireAttunement
    (-12i32) as u32, // AirWaterAttunement
    42264,           // DualAirAttunement
    (-13i32) as u32, // AirEarthAttunement
    (-14i32) as u32, // EarthFireAttunement
    (-15i32) as u32, // EarthWaterAttunement
    (-16i32) as u32, // EarthAirAttunement
    44857,           // DualEarthAttunement
];

/// Whether `id` is one of GW2EI's `SkillItem.IsSwap` ids -- the weapon-swap
/// sentinel plus the 4 non-sentinel categories: elementalist attunement
/// swaps, Weaver dual-attunement swaps, revenant legend swaps (5 variants),
/// necromancer shroud transforms (3 variants). This is a COMPLETE port of
/// EI's own `SkillItem.IsSwap` -- see this module's doc comment's "Extended
/// non-sentinel `is_swap` ids" and "Weaver's dual-attunement table"
/// sections for the full per-id citation.
pub fn is_swap(id: u32) -> bool {
    id == WEAPON_SWAP_SKILL_ID
        || ATTUNEMENT_SWAP_SKILL_IDS.contains(&id)
        || WEAVER_ATTUNEMENT_SWAP_SKILL_IDS.contains(&id)
        || LEGEND_SWAP_SKILL_IDS.contains(&id)
        || SHROUD_TRANSFORM_SKILL_IDS.contains(&id)
}

/// One skill's best-effort entry -- mirrors the native/ei-json schema shape
/// `{ name, auto_attack?, is_swap, can_crit }` (M14, Task 2). See this
/// module's doc comment for exactly what each field is (and is NOT)
/// derived from.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillMapEntry {
    /// Best-effort display name, from this log's own skill table. Falls
    /// back to `"Skill <id>"` when the log's own name for this id is empty
    /// or purely numeric.
    pub name: String,
    /// Always `None` -- deliberately omitted, not guessed. See this
    /// module's doc comment's "`auto_attack`: OMITTED, not guessed"
    /// section for the full citation.
    pub auto_attack: Option<bool>,
    /// `true` for [`WEAPON_SWAP_SKILL_ID`] plus the 4 non-sentinel
    /// categories [`is_swap`] checks (elementalist attunement swaps, Weaver
    /// dual-attunement swaps, revenant legend swaps, necromancer shroud
    /// transforms) -- a complete port of real EI's own `isSwap`. See this
    /// module's doc comment's "Extended non-sentinel `is_swap` ids"
    /// section.
    pub is_swap: bool,
    /// Reused verbatim from `hit_stats::can_crit` (M13's `NonCritableSkills`
    /// table).
    pub can_crit: bool,
}

/// Full best-effort skillMap: skill id -> [`SkillMapEntry`], scoped to only
/// the ids squad players actually reference (see this module's doc comment)
/// -- NOT a dump of the whole log skill table.
pub type SkillMap = BTreeMap<u32, SkillMapEntry>;

/// Resolves one id's display name from its (possibly absent) log-table
/// name: trims whitespace, then falls back to `"Skill <id>"` when the
/// trimmed result is empty OR every remaining character is an ASCII digit
/// (arcdps occasionally writes a bare numeric placeholder, or nothing, for
/// an id it had no cached display name for at capture time).
fn resolve_name(id: u32, raw_name: Option<&str>) -> String {
    let trimmed = raw_name.map(str::trim).unwrap_or("");
    let numeric_or_empty = trimmed.is_empty() || trimmed.chars().all(|c| c.is_ascii_digit());
    if numeric_or_empty {
        format!("Skill {id}")
    } else {
        trimmed.to_string()
    }
}

/// The referenced-id scope: every skill id any squad player's already-
/// computed `skill_damage`/`rotation` touches, plus the 12 always-tracked
/// boon ids. See this module's doc comment for the full "why these three
/// sources" writeup.
fn referenced_skill_ids(players: &[PlayerMetrics]) -> BTreeSet<u32> {
    let mut ids = BTreeSet::new();
    for p in players {
        for e in &p.skill_damage.outgoing {
            ids.insert(e.skill_id);
        }
        for e in &p.skill_damage.taken {
            ids.insert(e.skill_id);
        }
        for t in &p.skill_damage.per_target {
            for e in &t.skills {
                ids.insert(e.skill_id);
            }
        }
        for r in &p.rotation {
            ids.insert(r.skill_id);
        }
    }
    for &(id, _, _) in super::buffs::BOON_IDS.iter() {
        ids.insert(id);
    }
    ids
}

/// Builds the best-effort [`SkillMap`] for one analyzed log: the union of
/// every squad player's referenced skill ids (see `referenced_skill_ids`),
/// each resolved against `raw.skills` (the log's own decoded `cbtskill`
/// table). Called once, after every other per-player pass (`skill_damage`/
/// `rotation`) has already populated `players` -- mirrors
/// `Metrics::combat_participant_enemies`'s "computed from already-finished
/// per-player data" placement in `analyze()`.
pub fn build(raw: &RawLog, players: &[PlayerMetrics]) -> SkillMap {
    let ids = referenced_skill_ids(players);
    // Last-wins on a duplicate id (never observed in practice -- arcdps
    // writes one skill-table row per unique id -- but a plain `BTreeMap`
    // collect needs a defined tie-break rule regardless).
    let names: BTreeMap<u32, &str> = raw.skills.iter().map(|s| (s.id, s.name.as_str())).collect();
    ids.into_iter()
        .map(|id| {
            let entry = SkillMapEntry {
                name: resolve_name(id, names.get(&id).copied()),
                auto_attack: None,
                is_swap: is_swap(id),
                can_crit: hit_stats::can_crit(id),
            };
            (id, entry)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rotation::{Cast, SkillRotation};
    use crate::analysis::skill_damage::{PerTargetSkills, SkillDamageMetrics, SkillEntry};
    use crate::evtc::RawSkill;

    fn skill_entry(skill_id: u32) -> SkillEntry {
        SkillEntry { skill_id, total: 1, hits: 1, min: 1, max: 1, crit_hits: 0, flank_hits: 0 }
    }

    fn cast(cast_time_ms: i64) -> Cast {
        Cast { cast_time_ms, duration_ms: 100, time_gained_ms: 0, quickness: 0.0,
            status: crate::analysis::rotation::AnimationStatus::Full }
    }

    fn raw_with_skills(skills: Vec<RawSkill>) -> RawLog {
        use crate::evtc::{RawHeader, RawLog};
        RawLog { header: RawHeader { build: "20260114".into(), revision: 1, boss_id: 1 },
            agents: vec![], skills, events: vec![], guid_map: vec![] }
    }

    fn player_referencing(outgoing: &[u32], taken: &[u32], per_target: &[(u64, &[u32])], rotation: &[u32]) -> PlayerMetrics {
        PlayerMetrics {
            agent_addr: 1,
            skill_damage: SkillDamageMetrics {
                outgoing: outgoing.iter().map(|&id| skill_entry(id)).collect(),
                taken: taken.iter().map(|&id| skill_entry(id)).collect(),
                per_target: per_target
                    .iter()
                    .map(|&(enemy_id, ids)| PerTargetSkills { enemy_id, skills: ids.iter().map(|&id| skill_entry(id)).collect() })
                    .collect(),
            },
            rotation: rotation.iter().map(|&id| SkillRotation { skill_id: id, casts: vec![cast(0)] }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn named_skill_resolves_from_log_table() {
        let raw = raw_with_skills(vec![RawSkill { id: 5000, name: "Fireball".into() }]);
        let players = vec![player_referencing(&[5000], &[], &[], &[])];
        let map = build(&raw, &players);
        assert_eq!(map[&5000].name, "Fireball");
    }

    #[test]
    fn empty_name_falls_back_to_skill_id() {
        let raw = raw_with_skills(vec![RawSkill { id: 5001, name: "".into() }]);
        let players = vec![player_referencing(&[5001], &[], &[], &[])];
        let map = build(&raw, &players);
        assert_eq!(map[&5001].name, "Skill 5001");
    }

    #[test]
    fn whitespace_only_name_falls_back_to_skill_id() {
        let raw = raw_with_skills(vec![RawSkill { id: 5002, name: "   ".into() }]);
        let players = vec![player_referencing(&[5002], &[], &[], &[])];
        let map = build(&raw, &players);
        assert_eq!(map[&5002].name, "Skill 5002");
    }

    #[test]
    fn purely_numeric_name_falls_back_to_skill_id() {
        // arcdps sometimes writes a bare numeric placeholder string.
        let raw = raw_with_skills(vec![RawSkill { id: 5003, name: "27725".into() }]);
        let players = vec![player_referencing(&[5003], &[], &[], &[])];
        let map = build(&raw, &players);
        assert_eq!(map[&5003].name, "Skill 5003");
    }

    #[test]
    fn missing_skill_table_row_falls_back_to_skill_id() {
        // Referenced (via rotation) but absent from `raw.skills` entirely.
        let raw = raw_with_skills(vec![]);
        let players = vec![player_referencing(&[], &[], &[], &[6000])];
        let map = build(&raw, &players);
        assert_eq!(map[&6000].name, "Skill 6000");
    }

    #[test]
    fn name_is_trimmed() {
        let raw = raw_with_skills(vec![RawSkill { id: 5004, name: "  Meteor Shower  ".into() }]);
        let players = vec![player_referencing(&[5004], &[], &[], &[])];
        let map = build(&raw, &players);
        assert_eq!(map[&5004].name, "Meteor Shower");
    }

    #[test]
    fn weapon_swap_sentinel_is_flagged_is_swap() {
        let raw = raw_with_skills(vec![]);
        let players = vec![player_referencing(&[WEAPON_SWAP_SKILL_ID], &[], &[], &[])];
        let map = build(&raw, &players);
        assert!(map[&WEAPON_SWAP_SKILL_ID].is_swap);
    }

    #[test]
    fn ordinary_skill_is_not_flagged_is_swap() {
        let raw = raw_with_skills(vec![]);
        let players = vec![player_referencing(&[5005], &[], &[], &[])];
        let map = build(&raw, &players);
        assert!(!map[&5005].is_swap);
    }

    #[test]
    fn fire_attunement_swap_is_flagged_is_swap_and_independently_non_critable() {
        // M14 Task 3, Ruling B: 5492 (FireAttunementSkill) is a real,
        // confirmed divergence-of-independence case -- it's BOTH a swap
        // (elementalist attunement swap, the extended non-sentinel
        // is_swap category) AND non-critable (M13's NonCritableSkills
        // table, `hit_stats::NON_CRITABLE_SKILLS`), and the two flags must
        // both land correctly, independently of each other.
        let raw = raw_with_skills(vec![]);
        let players = vec![player_referencing(&[5492], &[], &[], &[])];
        let map = build(&raw, &players);
        assert!(map[&5492].is_swap, "5492 (FireAttunementSkill) must be flagged is_swap");
        assert!(!map[&5492].can_crit, "5492 (FireAttunementSkill) must remain non-critable");
    }

    #[test]
    fn revenant_legend_swap_and_necro_shroud_transform_are_flagged_is_swap() {
        // Spot-check one id per remaining curated category (Herald's own
        // legend-swap override, and Harbinger's shroud transform) so the
        // per-category id tables are each exercised, not just the base
        // Revenant/Necromancer entries.
        let raw = raw_with_skills(vec![]);
        let players = vec![player_referencing(&[28085, 62567], &[], &[], &[])];
        let map = build(&raw, &players);
        assert!(map[&28085].is_swap, "28085 (Herald's LegendaryDragonStanceSkill) must be flagged is_swap");
        assert!(map[&62567].is_swap, "62567 (EnterHarbingerShroud) must be flagged is_swap");
    }

    #[test]
    fn weaver_dual_attunement_swaps_are_flagged_is_swap() {
        // The 16-entry `_weaverAttunements` table splits into 4 real game
        // ids and 12 EI-invented negative pseudo ids; both halves must be
        // reachable, so spot-check one of each plus the two extremes of
        // the pseudo range (-5 and -16), which are the ones a sign/cast
        // slip would silently drop.
        let raw = raw_with_skills(vec![]);
        let ids: Vec<u32> = vec![43470, 44857, (-5i32) as u32, (-16i32) as u32];
        let players = vec![player_referencing(&ids, &[], &[], &[])];
        let map = build(&raw, &players);
        assert!(map[&43470].is_swap, "43470 (DualFireAttunement) must be flagged is_swap");
        assert!(map[&44857].is_swap, "44857 (DualEarthAttunement) must be flagged is_swap");
        assert!(map[&((-5i32) as u32)].is_swap, "-5 (FireWaterAttunement pseudo id) must be flagged is_swap");
        assert!(map[&((-16i32) as u32)].is_swap, "-16 (EarthAirAttunement pseudo id) must be flagged is_swap");
    }

    #[test]
    fn weaver_pseudo_ids_do_not_swallow_neighbouring_sentinels() {
        // EI's own SkillIDs block packs the Weaver pseudo range between
        // `NumberOfBoons = -3`/`NumberOfConditions = -4` above it and
        // `MirageCloakDodge = -17` below (`SkillIDs.cs:18-38`). None of
        // those three is an IsSwap id, so a range check written instead of
        // a membership check would wrongly flag them.
        assert!(!is_swap((-3i32) as u32), "-3 is not one of EI's IsSwap ids");
        assert!(!is_swap((-4i32) as u32), "-4 is not one of EI's IsSwap ids");
        assert!(!is_swap((-17i32) as u32), "-17 is past the Weaver pseudo range");
    }

    #[test]
    fn non_critable_skill_reuses_m13_table() {
        // 9292 = LightningStrike_SigilOfAir, one of `hit_stats::
        // NON_CRITABLE_SKILLS`'s 20 entries.
        let raw = raw_with_skills(vec![]);
        let players = vec![player_referencing(&[9292], &[], &[], &[])];
        let map = build(&raw, &players);
        assert!(!map[&9292].can_crit);
    }

    #[test]
    fn ordinary_skill_can_crit() {
        let raw = raw_with_skills(vec![]);
        let players = vec![player_referencing(&[5006], &[], &[], &[])];
        let map = build(&raw, &players);
        assert!(map[&5006].can_crit);
    }

    #[test]
    fn auto_attack_is_always_omitted() {
        let raw = raw_with_skills(vec![RawSkill { id: 5007, name: "Slash".into() }]);
        let players = vec![player_referencing(&[5007], &[], &[], &[])];
        let map = build(&raw, &players);
        assert_eq!(map[&5007].auto_attack, None);
    }

    #[test]
    fn scoping_covers_outgoing_taken_per_target_rotation_and_boons() {
        let raw = raw_with_skills(vec![]);
        let players = vec![player_referencing(&[1], &[2], &[(9, &[3])], &[4])];
        let map = build(&raw, &players);
        assert!(map.contains_key(&1), "outgoing skill id missing");
        assert!(map.contains_key(&2), "taken skill id missing");
        assert!(map.contains_key(&3), "per_target skill id missing");
        assert!(map.contains_key(&4), "rotation skill id missing");
        for &(boon_id, _, _) in super::super::buffs::BOON_IDS.iter() {
            assert!(map.contains_key(&boon_id), "boon id {boon_id} must always be included");
        }
    }

    #[test]
    fn unreferenced_skill_table_entry_is_excluded() {
        // A real skill in `raw.skills`, but never touched by any player's
        // damage/rotation/boons -- must NOT appear (this is a scoped map,
        // not a dump of the whole log skill table).
        let raw = raw_with_skills(vec![
            RawSkill { id: 1, name: "Referenced".into() },
            RawSkill { id: 999_999, name: "NeverTouched".into() },
        ]);
        let players = vec![player_referencing(&[1], &[], &[], &[])];
        let map = build(&raw, &players);
        assert!(map.contains_key(&1));
        assert!(!map.contains_key(&999_999));
    }

    #[test]
    fn empty_players_still_includes_the_12_boon_ids_only() {
        let raw = raw_with_skills(vec![]);
        let map = build(&raw, &[]);
        assert_eq!(map.len(), super::super::buffs::BOON_IDS.len());
    }
}
