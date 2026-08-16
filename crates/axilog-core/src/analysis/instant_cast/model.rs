//! Data model for GW2EI's `InstantCastFinder` subsystem.
//!
//! GW2EI expresses each finder as a C# object graph -- an abstract
//! `InstantCastFinder` subclass carrying closures. Every one of the 658
//! constructions in `EIData/ProfHelpers` is nevertheless a purely
//! DECLARATIVE builder chain: a constructor plus a fixed vocabulary of
//! `.WithBuilds(...)` / `.UsingOrigin(...)` / `.Using*Checker(...)` calls.
//! So this port represents a finder as DATA ([`FinderDef`]) and evaluates
//! it with one engine ([`super::compute`]), exactly the way
//! `analysis::damage_mods` represents GW2EI's `DamageModifier` graph.
//!
//! That choice is what makes the catalog machine-extractable
//! (`scripts/gen_instant_cast_catalog.py`) rather than hand-transcribed,
//! and it is why anything the vocabulary below cannot express is REJECTED
//! by the generator with a named reason instead of being approximated.

/// `InstantCastFinder.InstantCastOrigin` (`InstantCastFinder.cs:12-18`).
///
/// This is the whole reason MPROC exists: `SkillData.cs:44-58` turns the
/// origin of every AVAILABLE finder into the `skillMap` booleans
/// `isTraitProc` / `isGearProc` / `isUnconditionalProc`. [`Skill`] is the
/// default and sets no flag.
///
/// [`Skill`]: CastOrigin::Skill
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CastOrigin {
    Skill,
    Trait,
    Gear,
    Unconditional,
}

/// GW2EI `InstantCastFinder.DefaultICD` (`InstantCastFinder.cs:26`).
pub const DEFAULT_ICD: i64 = 50;

/// GW2EI `ParserHelper.ServerDelayConstant`.
pub const SERVER_DELAY: i64 = 150;

/// GW2EI `GW2Builds.StartOfLife` / `EndOfLife` stand-ins, and the same for
/// `ArcDPSBuilds`. Deliberately identical to the constants
/// `analysis::damage_mods::model` already defines -- both subsystems gate
/// on the same two builds with the same half-open `[min, max)` rule -- but
/// re-declared here so the catalog files need only one import.
pub const START_OF_LIFE: u64 = 0;
/// See [`START_OF_LIFE`].
pub const END_OF_LIFE: u64 = u64::MAX;
/// See [`START_OF_LIFE`].
pub const EVTC_START_OF_LIFE: i64 = i64::MIN;
/// See [`START_OF_LIFE`].
pub const EVTC_END_OF_LIFE: i64 = i64::MAX;

/// Which event stream a finder watches, and how the CASTER is recovered
/// from a matching event.
///
/// One variant per `InstantCastFinder` subclass that this port evaluates.
/// The subclasses are near-identical in structure -- group the stream by
/// caster, drop events inside the finder's ICD, emit one instant cast per
/// survivor -- so the variance really is only "which stream" and "which
/// field is the caster", which is exactly what this enum encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// `BuffGainCastFinder` (190 constructions -- the largest bucket that
    /// does not need effect data). Watches applications of `buff_id`; the
    /// caster is the buff's RECIPIENT (`BuffGainCastFinder.cs:13-16`,
    /// `GetKeyAgent => evt.To`), because "I gained this buff" is the
    /// observable trace of "I cast the thing that grants it".
    ///
    /// The ctor installs a permanent `!bae.Initial` checker
    /// (`:11`): a stack that existed before the log started is not
    /// evidence of a cast inside the log.
    BuffGain { buff_id: u32 },
    /// `BuffLossCastFinder` (33). Watches `BUFF_REMOVE_ALL` of `buff_id`
    /// on the caster (`BuffLossCastFinder.cs:10-13`, `GetKeyAgent =>
    /// evt.To`). Note the deliberate asymmetry with [`BuffGain`]: there is
    /// no `Initial` equivalent to exclude, so the ctor installs no checker.
    ///
    /// [`BuffGain`]: Trigger::BuffGain
    BuffLoss { buff_id: u32 },
    /// `BuffGiveCastFinder` (21). Watches applications of `buff_id` but
    /// takes the APPLIER as the caster (`BuffGiveCastFinder.cs:16-19`,
    /// `GetKeyAgent => evt.By`) -- the "I buffed someone else" direction.
    /// Always minion-folded (`:13`, and `WithMinions` throws), so the
    /// applier is resolved through to its final master.
    BuffGive { buff_id: u32 },
    /// `BuffExtendCastFinder`. Watches duration EXTENSIONS of `buff_id` on
    /// the caster (`BuffExtendCastFinder.cs:13-16`).
    BuffExtend { buff_id: u32 },
    /// `DamageCastFinder` (77). Watches health-damage events whose skill
    /// is `skill_id`; the caster is the damage SOURCE. The ctor forces
    /// `UsingNotAccurate()` (`DamageCastFinder.cs:9`) -- damage lands some
    /// time after the cast, so the recovered timestamp is an upper bound,
    /// not the cast instant. Every `DamageCastFinder` is therefore an
    /// `isNotAccurate` skill.
    Damage { skill_id: u32 },
    /// `BreakbarDamageCastFinder` (2). As [`Damage`], over breakbar-damage
    /// events, and equally `UsingNotAccurate()`.
    ///
    /// [`Damage`]: Trigger::Damage
    BreakbarDamage { skill_id: u32 },
    /// `MinionCommandCastFinder` (86). A `BuffGainCastFinder` over the
    /// shared "minion command" buff, narrowed to recipients of one species
    /// that have a master (`MinionCommandCastFinder.cs:22`). Always
    /// minion-folded, so the emitted caster is the commanding player.
    MinionCommand { species_id: u32 },
    /// `MinionCastCastFinder` (12). Watches ANIMATED casts of
    /// `skill_id` by an agent that has a master; the caster is that
    /// master (`MinionCastCastFinder.cs:19-33`).
    MinionCast { skill_id: u32 },
    /// `MinionSpawnCastFinder` (11). Watches spawns of any of
    /// `species_ids` that have a master; the caster is the master.
    MinionSpawn { species_ids: &'static [u32] },
    /// `MissileCastFinder` (11). Watches missile creations of
    /// `skill_id`; the caster is the missile's source.
    Missile { skill_id: u32 },
    /// `EXTHealingCastFinder` (32). Watches healing-extension events of
    /// `skill_id`; the caster is the healer. `UsingNotAccurate()` and
    /// enabled only when the log carries the healing extension.
    ///
    /// **The ICD test runs BEFORE the checkers here**, the opposite order
    /// to every other subclass (`EXTHealingCastFinder.cs:30-40` vs e.g.
    /// `DamageCastFinder.cs:23-31`). That is not a transcription slip; it
    /// is reproduced faithfully by [`Trigger::icd_before_checks`].
    ExtHealing { skill_id: u32 },
}

impl Trigger {
    /// Whether this subclass applies its ICD gate BEFORE evaluating its
    /// checkers. Only the two healing/barrier extension finders do; see
    /// [`Trigger::ExtHealing`].
    ///
    /// The distinction is observable: with checks-first, an event that
    /// FAILS a checker never advances `lastTime`, so it cannot suppress a
    /// later passing event; with ICD-first it can.
    pub fn icd_before_checks(&self) -> bool {
        matches!(self, Trigger::ExtHealing { .. })
    }

    /// Whether the subclass ctor forces minion-folding of the caster --
    /// see [`FinderDef::minions`].
    pub fn forces_minions(&self) -> bool {
        matches!(
            self,
            Trigger::BuffGive { .. }
                | Trigger::MinionCommand { .. }
                | Trigger::MinionCast { .. }
                | Trigger::MinionSpawn { .. }
        )
    }

    /// Whether the subclass ctor forces `UsingNotAccurate()`.
    pub fn forces_not_accurate(&self) -> bool {
        matches!(
            self,
            Trigger::Damage { .. } | Trigger::BreakbarDamage { .. } | Trigger::ExtHealing { .. }
        )
    }
}

/// Log-level preconditions -- GW2EI's `_enableConditions`
/// (`InstantCastFinder.cs:81-95`), ANDed inside `Available`.
///
/// These are the reason a static `(gw2_build, evtc_build) -> flags` table
/// CANNOT reproduce EI's `skillMap` output: availability depends on what
/// the log contains, not only on the build pair it was recorded at. The
/// two that matter in practice are the effect-data pair -- a finder that
/// reconstructs a cast from a buff side effect is DISABLED on logs that
/// carry the effect events themselves, because a dedicated effect finder
/// covers those logs better.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enable {
    /// `.UsingDisableWithEffectData()` (`:87-90`) -- `!HasEffectData`.
    NoEffectData,
    /// The implicit condition every `EffectCastFinder` ctor installs
    /// (`EffectCastFinder.cs:41`) -- `HasEffectData`.
    HasEffectData,
    /// `.UsingDisableWithMissileData()` (`:92-95`) -- `!HasMissileData`.
    NoMissileData,
    /// `EXTHealingCastFinder.cs:13` -- `HasEXTHealing`.
    HasExtHealing,
}

/// Which side of a triggering event a [`Check`] reads.
///
/// GW2EI uses two naming pairs for the same idea -- `To`/`By` on buff
/// events, `Src`/`Dst` on effect and damage events -- where the first
/// named party is always the one the subclass's `GetKeyAgent` returns.
/// [`Key`] is that party; [`Other`] is its counterpart.
///
/// [`Key`]: Party::Key
/// [`Other`]: Party::Other
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Party {
    /// The subclass's `GetKeyAgent` -- the buff's RECIPIENT on an apply,
    /// the damage SOURCE on a hit.
    Key,
    /// The counterpart -- the APPLIER on a buff apply, the victim on a
    /// hit.
    Other,
}

/// Per-event predicates -- GW2EI's `CheckedCastFinder._checkers`
/// (`CheckedCastFinder.cs:8-24`), ANDed.
///
/// Only the shapes the builder vocabulary expresses declaratively appear
/// here. A bare `.UsingChecker(lambda)` is an arbitrary closure over the
/// parsed log and is NOT representable; the generator rejects such a
/// finder rather than dropping the condition, because dropping it would
/// silently WIDEN the finder and produce casts EI never emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// `.UsingToSpecChecker(spec)` / `.UsingBySpecChecker(spec)` and their
    /// base-spec and negated forms.
    ///
    /// `party` is which side of the triggering event the spec is read
    /// from -- GW2EI names the two `To`/`By` on a buff event and
    /// `Src`/`Dst` on an effect event, which [`Party`] unifies. It cannot
    /// be inferred from the [`Trigger`]: `BuffGiveCastFinder` finders read
    /// BOTH (`UsingToSpecChecker` on the recipient, `UsingBySpecChecker`
    /// on the applier that is also the caster).
    ///
    /// `base` selects GW2EI's `GetBaseSpecAtTime` (the core profession)
    /// over `GetSpecAtTime` (the elite spec when one is equipped).
    Spec { party: Party, spec: &'static str, base: bool, negated: bool },
    /// `.UsingDurationChecker(duration)` on a buff apply/extend finder:
    /// `|applied - duration| < epsilon` (`BuffGainCastFinder.cs:18-22`).
    Duration { duration: i64, epsilon: i64 },
}

/// One transcribed `InstantCastFinder`.
///
/// The field set is the union of what `InstantCastFinder`'s builder
/// methods can set, minus the ones no ProfHelper construction uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinderDef {
    /// The skill this finder CLAIMS was cast -- `InstantCastFinder.SkillID`
    /// and the key the `skillMap` flags are filed under. NOT the id of the
    /// event that triggered the finder, which lives in [`FinderDef::trigger`].
    pub skill_id: u32,
    /// The GW2EI helper file this was extracted from, e.g.
    /// `"GuardianHelper"`. Carried for provenance only: it makes a
    /// generated catalog row traceable back to its source line without a
    /// separate index, and lets a test assert coverage per profession.
    pub source: &'static str,
    pub trigger: Trigger,
    /// `.UsingOrigin(...)`, defaulting to [`CastOrigin::Skill`].
    pub origin: CastOrigin,
    /// `.UsingNotAccurate()`, ORed with the subclass ctor's own forcing
    /// (see [`Trigger::forces_not_accurate`]). Feeds `skillMap`'s
    /// `isNotAccurate`.
    pub not_accurate: bool,
    /// `.WithMinions()` (`BuffCastFinder.cs:21-25`,
    /// `MissileCastFinder.cs:14-18`): resolve the caster through
    /// `GetFinalMaster()`, so a pet's buff/missile is credited to its
    /// owner. Forced on for [`Trigger::BuffGive`],
    /// [`Trigger::MinionCommand`], [`Trigger::MinionCast`] and
    /// [`Trigger::MinionSpawn`], whose ctors set it and whose
    /// `WithMinions` overrides throw.
    pub minions: bool,
    /// `.UsingICD(icd)`, defaulting to [`DEFAULT_ICD`].
    pub icd: i64,
    /// `.UsingTimeOffset(offset)` -- added to the triggering event's time.
    pub time_offset: i64,
    /// ANDed log-level preconditions.
    pub enable: &'static [Enable],
    /// ANDed per-event predicates.
    pub checks: &'static [Check],
    /// `.WithBuilds(min, max)` -- half-open `[min, max)` over the GW2 build.
    pub min_gw2_build: u64,
    /// See [`FinderDef::min_gw2_build`].
    pub max_gw2_build: u64,
    /// `.WithEvtcBuilds(min, max)` -- half-open, over the arcdps build.
    pub min_evtc_build: i64,
    /// See [`FinderDef::min_evtc_build`].
    pub max_evtc_build: i64,
}

impl FinderDef {
    /// A finder with everything at its GW2EI default. Catalog rows are
    /// written as `FinderDef { skill_id: .., trigger: .., ..DEFAULT }`,
    /// which keeps a generated row to the fields its source line actually
    /// sets.
    pub const DEFAULT: FinderDef = FinderDef {
        skill_id: 0,
        source: "",
        trigger: Trigger::BuffGain { buff_id: 0 },
        origin: CastOrigin::Skill,
        not_accurate: false,
        minions: false,
        icd: DEFAULT_ICD,
        time_offset: 0,
        enable: &[],
        checks: &[],
        min_gw2_build: START_OF_LIFE,
        max_gw2_build: END_OF_LIFE,
        min_evtc_build: EVTC_START_OF_LIFE,
        max_evtc_build: EVTC_END_OF_LIFE,
    };

    /// `.UsingNotAccurate()` ORed with the subclass ctor's forcing --
    /// what `skillMap`'s `isNotAccurate` reads.
    pub fn is_not_accurate(&self) -> bool {
        self.not_accurate || self.trigger.forces_not_accurate()
    }

    /// GW2EI `InstantCastFinder.Available` (`:138-154`).
    ///
    /// Both build tests are half-open on the right (`build < max`) and
    /// closed on the left (`build >= min`) -- a reworked skill becomes two
    /// finders with adjacent, non-overlapping ranges, so an inclusive
    /// upper bound would double-count at the seam.
    ///
    /// A log with no `CBTS_GWBUILD` row reads as [`START_OF_LIFE`], which
    /// matches GW2EI: its `GetGW2BuildEvent()` synthesises a
    /// `StartOfLife` build event when the log carries none.
    pub fn available(&self, log: &LogCapabilities) -> bool {
        if !self.enable.iter().all(|c| log.satisfies(*c)) {
            return false;
        }
        let gw2 = log.gw2_build.unwrap_or(START_OF_LIFE);
        let evtc = log.evtc_build.unwrap_or(EVTC_START_OF_LIFE);
        gw2 >= self.min_gw2_build
            && gw2 < self.max_gw2_build
            && evtc >= self.min_evtc_build
            && evtc < self.max_evtc_build
    }
}

/// The log-level facts [`Enable`] conditions and build gates read.
///
/// Bundled into one struct rather than passed as four arguments so that
/// adding a future enable condition (there are only a handful in GW2EI)
/// does not churn every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LogCapabilities {
    /// `CBTS_GWBUILD`; `None` for a log without one.
    pub gw2_build: Option<u64>,
    /// The arcdps build from the EVTC header, `None` if unparseable.
    pub evtc_build: Option<i64>,
    /// Whether the log carries effect events at all -- GW2EI's
    /// `CombatData.HasEffectData`.
    pub has_effect_data: bool,
    /// GW2EI's `CombatData.HasMissileData`.
    pub has_missile_data: bool,
    /// GW2EI's `CombatData.HasEXTHealing` -- the healing extension's
    /// registration event is present.
    pub has_ext_healing: bool,
}

impl LogCapabilities {
    fn satisfies(&self, c: Enable) -> bool {
        match c {
            Enable::NoEffectData => !self.has_effect_data,
            Enable::HasEffectData => self.has_effect_data,
            Enable::NoMissileData => !self.has_missile_data,
            Enable::HasExtHealing => self.has_ext_healing,
        }
    }
}

/// One recovered instant cast -- GW2EI's `InstantCastEvent`.
///
/// GW2EI's own type carries a resolved `SkillItem` and `AgentItem`; this
/// one carries the ids, because every consumer in this project
/// (`skill_map`'s flags, `rotation`'s cast list) resolves them itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstantCastEvent {
    /// Log-absolute arcdps time, in the same clock as `RawEvent::time`.
    pub time: u64,
    /// The CAST skill -- [`FinderDef::skill_id`], not the trigger's id.
    pub skill_id: u32,
    /// Raw agent addr of the caster.
    pub caster: u64,
}
