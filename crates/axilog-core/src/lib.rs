pub mod analysis;
pub mod evtc;
/// Elite-Insights-matching icon URL tables (M15 Task 2) -- currently the
/// profession / elite-spec icons the combat replay exports as
/// `combatReplayData.iconURL`. See [`icons`]' module doc.
pub mod icons;
pub mod model;
/// Encounter identity for non-WvW logs (name, category, success) -- see the
/// module doc for why the general rule needs no table and what it does NOT
/// cover (challenge motes, per-encounter success, target selection).
pub mod pve;
pub mod wvw;
