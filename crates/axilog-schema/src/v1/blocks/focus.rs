//! `blocks.focus` -- how much of the enemy's attention each squad player
//! drew, from the enemy cast-start census arcdps's enemy-event filter
//! leaves in the log.
//!
//! See [`axilog_core::analysis::focus`]' module doc for why the filter
//! makes this measurable at all, for the 1,400-log holdout that validated
//! `focus_index`, and for the damage-weighting variant that was built,
//! measured, and rejected -- do not re-derive it from this block's
//! `skills` rows without reading that first.
//!
//! ## Always-on, like `squad_buffs`
//!
//! The pass is one linear scan of the raw event list with no allocation
//! per event, so no flag gates it. `None` from the caller therefore means
//! "no log to scan" -- a reprojection driven from a `Report` alone -- not
//! a gate that was off, and `coverage.focus` reports `not_computed` for
//! exactly that case rather than an empty block a consumer would read as
//! "nobody was targeted".
//!
//! ## Absent entirely on a pre-rework log
//!
//! The enemy cast census only exists on arcdps builds from 2026-05
//! onwards -- 56% of a 4,143-log WvW corpus predates it and carries no such
//! row at all, while carrying millions of enemy->squad strike rows in the
//! same file. There is nothing for any flag or future pass to recover, so
//! this block is OMITTED and `coverage.focus` reads `unsupported`, the same
//! answer `healing` gives for a log recorded without the healing extension.
//! `empty` is reserved for a log that COULD have carried the census and did
//! not -- a genuinely quiet fight.
//!
//! A consumer must therefore treat a missing `blocks.focus` as "not
//! measurable here", never as "nobody was targeted".
//!
//! ## Why the block is near-empty on a PvE log
//!
//! [`axilog_core::analysis::focus`] counts casts from enemy **players**
//! only -- an NPC does not choose a target the way a player does, and
//! folding boss casts in here would make the index measure boss scripting
//! rather than enemy intent. On a raid or fractal log that leaves every
//! row at zero, which `coverage` reports as `empty`.

use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::EntityIndex;
use axilog_core::analysis::focus::FocusDetail;
use serde::Serialize;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct FocusBlock {
    /// Squad players counted in the denominator of [`FocusEntity::focus_index`]
    /// -- squad members only. A non-squad friendly in the log gets neither a
    /// row here nor a place in this count.
    pub squad_size: u32,
    /// Enemy cast-starts aimed at any squad member, the total
    /// [`FocusEntity::casts_drawn`] is a share of.
    pub total_casts: u64,
    /// Enemy cast-starts aimed at squad MINIONS -- pets, clones, phantasms,
    /// spirit weapons, turrets, gyros. 8.3% of the census on a 4,143-log WvW
    /// corpus. Not part of [`Self::total_casts`] and not scored into
    /// [`FocusEntity::focus_index`]; see the core module doc for the holdout
    /// that decided that.
    pub total_minion_casts: u64,
    /// The window, in ms before a down, that [`FocusEntity::pre_down_casts`]
    /// counts casts in. Carried so a consumer reporting "casts in the N
    /// seconds before going down" reads N from the document rather than
    /// hardcoding a constant that a later axilog could change under it.
    pub pre_down_window_ms: u64,
    /// Mean damage of one connecting enemy strike on a squad member in this
    /// log, across every skill. The scale [`FocusSkill`]'s damage is read
    /// against -- "3x the mean strike" is meaningful, "8000 damage" is not
    /// without knowing the fight.
    ///
    /// Strike damage only: condition ticks, and the `CROWD_CONTROL` /
    /// `BREAKBAR_DAMAGE` results that carry a defiance number rather than a
    /// health number, are excluded.
    pub mean_strike_damage: f64,
    /// Per-skill diagnostics for what was aimed at the squad, ascending by
    /// skill id. Ids resolve through `catalogs.skills`.
    ///
    /// **Pool these across logs before drawing conclusions.** The median
    /// enemy skill connects three times in a single WvW log, which is not
    /// a sample you can take a mean of -- which is why each row carries
    /// `hits` and `damage_total` rather than a mean. Two logs' `(hits,
    /// damage_total)` pairs add; their means do not.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<FocusSkill>,
    pub by_entity: ByEntity<FocusEntity>,
}

impl FocusBlock {
    /// Empty when nothing was aimed at the squad, regardless of how many
    /// rows exist: this block emits a zeroed row for every squad player, so
    /// a row count would report `present` on a log with no enemy players in
    /// it at all.
    pub fn is_empty(&self) -> bool {
        self.total_casts == 0
    }
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct FocusEntity {
    /// Enemy cast-starts whose target was this player.
    pub casts_drawn: u64,
    /// Enemy cast-starts whose target was one of this player's minions,
    /// attributed through the row's `dst_master_instid`. A separate axis
    /// from [`Self::casts_drawn`], never summed into it.
    pub casts_drawn_minions: u64,
    /// This player's share of [`FocusBlock::total_casts`], divided by an
    /// even `1/squad_size` share. `1.0` is exactly average attention, `3.0`
    /// is three times what an evenly-targeted squad member would draw.
    ///
    /// Zero for every player when `total_casts` is zero -- there is no
    /// share of nothing.
    pub focus_index: f64,
    /// Times this player entered downstate.
    pub downs: u64,
    /// Casts aimed at this player inside [`FocusBlock::pre_down_window_ms`]
    /// before one of their downs.
    ///
    /// Windows around successive downs may OVERLAP, and a cast inside two
    /// of them counts twice -- this answers "how much attention preceded a
    /// down", so a player downed twice in four seconds under sustained fire
    /// should read high, not be deduplicated down to average.
    pub pre_down_casts: u64,
}

/// One enemy skill's activity against the squad.
///
/// The two halves are measured on DIFFERENT event streams and neither
/// bounds the other. `casts_at_squad` counts cast-STARTS aimed at a squad
/// member; `hits`/`damage_total` count damage events that landed. A skill
/// can therefore have casts and no hits (every one missed, blocked, or was
/// aimed at someone who moved), or hits and no casts (an instant with no
/// animation, or a projectile that connected after its caster's start row
/// fell outside the log window).
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct FocusSkill {
    /// `catalogs.skills` key.
    pub skill: u32,
    pub casts_at_squad: u64,
    /// Connecting strikes on squad members. Same exclusions as
    /// [`FocusBlock::mean_strike_damage`].
    pub hits: u64,
    pub damage_total: u64,
}

/// Reproject the pass onto entity ids.
///
/// Positionally joined to `report.players`, the same join
/// `activity::build_series` uses for `entity_series` and for the same
/// reason: [`FocusDetail`] is a `Vec` over `enc.players` with no addr in
/// it, so a length that disagrees with this report's roster would
/// misattribute every row rather than fail. A mismatch drops the whole
/// pass -- absent means "not measured", which is what a consumer needs to
/// hear.
pub fn build_focus(
    detail: &FocusDetail,
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
) -> FocusBlock {
    if detail.len() != report.players.len() {
        return FocusBlock::default();
    }
    let mut by_entity = ByEntity::default();
    for (i, p) in report.players.iter().enumerate() {
        // Squad only, matching `squad_size`. A non-squad friendly is never
        // in the denominator, so emitting its (always zero) row here would
        // read as "was in the squad and drew no attention" -- a claim this
        // pass did not measure.
        if !p.in_squad {
            continue;
        }
        let Some(id) = index.by_agent_addr(p.agent_addr) else { continue };
        let f = detail.at(i);
        by_entity.insert(
            id,
            FocusEntity {
                casts_drawn: f.casts_drawn,
                casts_drawn_minions: f.casts_drawn_minions,
                focus_index: f.focus_index,
                downs: f.downs,
                pre_down_casts: f.pre_down_casts,
            },
        );
    }
    let skills = detail
        .skills
        .values()
        .map(|t| {
            cats.reference_skill(t.skill_id);
            FocusSkill {
                skill: t.skill_id,
                casts_at_squad: t.casts_at_squad,
                hits: t.hits,
                damage_total: t.damage_total,
            }
        })
        .collect();
    FocusBlock {
        squad_size: detail.squad_size as u32,
        total_casts: detail.total_casts,
        total_minion_casts: detail.total_minion_casts,
        pre_down_window_ms: axilog_core::analysis::focus::PRE_DOWN_WINDOW_MS,
        mean_strike_damage: detail.mean_strike_damage,
        skills,
        by_entity,
    }
}
