//! Data model for GW2EI's `InstantCastFinder` subsystem.
//!
//! GW2EI expresses each finder as a C# object graph -- an abstract
//! `InstantCastFinder` subclass carrying closures. Every one of the 649
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
    /// `EffectCastFinder` (112) and `EffectCastFinderByDst` (60) -- the
    /// largest bucket of all. Watches the visual effects the log carries
    /// (`crate::evtc::effect`), matched by STABLE GUID rather than by the
    /// session-local effect id the wire actually names.
    ///
    /// For a great many traits and sigils the spawned visual is the only
    /// trace in the log that the thing fired at all, which is why this
    /// bucket is so large -- and why it stayed unimplemented for so long,
    /// since it needs the effect decode these finders sit on top of.
    ///
    /// `by_dst` selects the `EffectCastFinderByDst` subclass, which swaps
    /// the two agents: the caster is the agent the effect is ANCHORED to
    /// rather than the one that spawned it. A ground-anchored effect has
    /// no such agent, so a `by_dst` finder never fires on one (GW2EI drops
    /// the group whose key `IsUnknown`).
    ///
    /// The ctor forces both `UsingNotAccurate()` and the `HasEffectData`
    /// enable condition (`EffectCastFinder.cs:39-43`).
    Effect { guid: &'static [u8; 16], by_dst: bool },
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
            Trigger::Damage { .. }
                | Trigger::BreakbarDamage { .. }
                | Trigger::ExtHealing { .. }
                | Trigger::Effect { .. }
        )
    }

    /// Whether the subclass ctor installs the `HasEffectData` enable
    /// condition. Modelled here rather than emitted into every effect
    /// row's [`FinderDef::enable`] so a catalog row cannot omit it.
    pub fn forces_has_effect_data(&self) -> bool {
        matches!(self, Trigger::Effect { .. })
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

/// `.UsingBeforeWeaponSwap()` / `.UsingAfterWeaponSwap()`
/// (`InstantCastFinder.cs:103-115`), consumed by `GetTime` (`:117-129`).
///
/// A skill that forces a weapon/bundle swap -- leaving a shroud, stowing
/// a tome, sheathing a gunsaber -- produces its trace and its swap within
/// a few frames of each other, in an order the wire does not guarantee.
/// These pull the emitted cast to the correct SIDE of the swap so the
/// rotation reads in the order the player actually played it.
///
/// The snap applies only when a swap by the same caster falls inside
/// `ServerDelayConstant / 2` of the candidate time; otherwise the time is
/// unchanged. 28 finders request it, all `Before`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapSnap {
    /// No snapping -- the GW2EI default.
    None,
    /// `min(swap_time - 1, time)`.
    Before,
    /// `max(swap_time + 1, time)`.
    After,
}

/// Which side of a triggering EVENT a [`Check`] reads.
///
/// GW2EI uses two naming pairs for the same idea -- `To`/`By` on buff
/// events, `Src`/`Dst` on effect and damage events. [`Key`] is always the
/// first of each pair (`To`, `Src`) and [`Other`] the second (`By`,
/// `Dst`).
///
/// Note that this is a property of the EVENT, not of the finder: which of
/// the two the finder calls its caster is the subclass's business
/// (`BuffGiveCastFinder` takes the applier, `EffectCastFinderByDst` the
/// anchor agent), and a checker naming `Src` means `Src` regardless. Two
/// finders on opposite sides of the same stream therefore agree about
/// what [`Key`] means, which is what lets them share a collected stream.
///
/// [`Key`]: Party::Key
/// [`Other`]: Party::Other
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Party {
    /// The buff's RECIPIENT on an apply, the damage SOURCE on a hit, the
    /// effect's SPAWNER.
    Key,
    /// The APPLIER on a buff apply, the victim on a hit, the agent an
    /// effect is anchored to.
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
    ///
    /// `specs` is a SET because GW2EI has both singular and plural forms
    /// (`UsingSrcSpecChecker` / `UsingSrcSpecsChecker`); a singular check
    /// is a one-element set. Passing means "the party's spec is in the
    /// set", XORed with `negated`.
    Spec { party: Party, specs: &'static [&'static str], base: bool, negated: bool },
    /// `.UsingDurationChecker(duration)` on a buff apply/extend finder:
    /// `|applied - duration| < epsilon` (`BuffGainCastFinder.cs:18-22`).
    Duration { duration: i64, epsilon: i64 },
    /// `.UsingIsAroundDstChecker()` / `.UsingNotIsAroundDstChecker()`
    /// (`EffectCastFinder.cs:113-123`) -- whether the effect follows an
    /// agent or sits at a fixed map position.
    ///
    /// Also emitted implicitly alongside every Dst-side [`Check::Spec`] on
    /// an effect finder, because GW2EI's `UsingDst*SpecChecker` bodies all
    /// begin `evt.IsAroundDst &&` (`EffectCastFinder.cs:51-55, 88-110`).
    /// Making that explicit in the catalog keeps the engine from having to
    /// infer it from the pairing.
    AroundDst { negated: bool },
    /// `.UsingDurationChecker(d)` / `.UsingDurationChecker(min, max)` on an
    /// EFFECT finder (`EffectCastFinder.cs:125-135`).
    ///
    /// Deliberately NOT [`Check::Duration`]: the buff form is an epsilon
    /// band around a target (`|applied - d| < epsilon`), while this one is
    /// exact equality, or an INCLUSIVE `[min, max]` range in the two-
    /// argument form. Folding them would quietly widen one or narrow the
    /// other.
    EffectDuration { min: i64, max: i64 },
    /// The `Using[No]SecondaryEffect*Checker` family
    /// (`EffectCastFinder.cs:137-260`) -- twelve builder methods that are
    /// one predicate with three independent switches.
    ///
    /// "Did another effect with GUID `guid`, belonging to the same caster,
    /// happen within `epsilon` of `time + time_offset`?" A compound skill
    /// spawns several visuals at once, and the combination is what
    /// identifies it; either half alone is ambiguous.
    SecondaryEffect {
        guid: &'static [u8; 16],
        /// Which of the OTHER effect's agents must equal this finder's
        /// caster: its own key agent (`false`, GW2EI's `GetAgent(other)`)
        /// or its counterpart (`true`, `GetOtherAgent(other)`).
        inverted_src: bool,
        /// Whether the other effect must be anchored the same way as this
        /// one, the opposite way, or either.
        type_rel: TypeRel,
        time_offset: i64,
        epsilon: i64,
        /// The `UsingNoSecondaryEffect*` half of the family. Note that a
        /// negated check passes when the log has NO effects under `guid`
        /// at all -- GW2EI's `else return true` arm -- which the positive
        /// form's empty-set `false` mirrors.
        negated: bool,
    },
}

/// How a [`Check::SecondaryEffect`]'s other effect must be anchored
/// relative to the triggering one -- GW2EI's `IsAroundDst` comparison,
/// which appears in three forms across the twelve builder methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeRel {
    /// No comparison (`UsingSecondaryEffectSameSrcChecker` and friends).
    Any,
    /// `evt.IsAroundDst == other.IsAroundDst`.
    Same,
    /// `evt.IsAroundDst != other.IsAroundDst`.
    Inverted,
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
    /// Weapon-swap time snapping; see [`SwapSnap`].
    pub swap_snap: SwapSnap,
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
        swap_snap: SwapSnap::None,
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
        if self.trigger.forces_has_effect_data() && !log.has_effect_data {
            return false;
        }
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
