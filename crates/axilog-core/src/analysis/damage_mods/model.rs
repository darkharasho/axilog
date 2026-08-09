//! The damage-modifier DEFINITION model (M16, Task 1) -- a data-only
//! transliteration of GW2EI's `DamageModifierDescriptor` +
//! `DamageModifier` pair.
//!
//! GW2EI splits the two because the same descriptor list is instantiated
//! once as an outgoing modifier and once as an incoming one
//! (`GW2EIEvtcParser/EIData/DamageModifiers/DamageModifier.cs:9-33`: nearly
//! every property on `DamageModifier` is a `=>` forward to
//! `DamageModDescriptor`, the only genuinely new state being `Incoming` and
//! the direction-dependent `GetDamageEvents`/`GetTotalDamage`/`GetFoe`/
//! `GetActor`). Since a definition's direction is already implied by its
//! [`DamageSource`] (`DamageSource.Incoming` <=> incoming -- GW2EI asserts
//! exactly that invariant in both ctors, `OutgoingDamageModifier.cs:9-16`
//! and `IncomingDamageModifier.cs:8-15`), one flat struct carries both here.

/// GW2EI `DamageType` (`GW2EIEvtcParser/ParserHelpers/ParserHelper.cs:84-95`).
///
/// Declared `[Flags]` there (`All=0, Power=1, Strike=2, Condition=4,
/// LifeLeech=8`), but `Actor.FilterDamageEvents`
/// (`GW2EIEvtcParser/EIData/Actors/Actor.cs:446-479`) switches on the WHOLE
/// value and throws `NotImplementedException` on anything outside the nine
/// combinations below -- so this is an enum of exactly those nine, not a
/// bitset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageType {
    /// No filtering at all (`Actor.cs:476`, the `All` arm is a no-op).
    All,
    /// `RemoveAll(x => x.ConditionDamageBased(log))` (`Actor.cs:451`).
    Power,
    /// `RemoveAll(x => x is NonDirectHealthDamageEvent)` (`Actor.cs:454`) --
    /// i.e. `buff == 0` rows only.
    Strike,
    /// `RemoveAll(x => !x.ConditionDamageBased(log))` (`Actor.cs:460`).
    Condition,
    /// `RemoveAll(x => !x.IsLifeLeech)` (`Actor.cs:457`).
    LifeLeech,
    /// `Actor.cs:463`.
    StrikeAndCondition,
    /// `Actor.cs:466`.
    ConditionAndLifeLeech,
    /// `Actor.cs:469`.
    StrikeAndLifeLeech,
    /// `Actor.cs:472`.
    StrikeAndConditionAndLifeLeech,
}

impl DamageType {
    /// Does a hit of this shape survive the filter?
    ///
    /// `is_strike` is GW2EI's `!(x is NonDirectHealthDamageEvent)` (a
    /// `buff == 0` row), `is_condition` its `x.ConditionDamageBased(log)`
    /// (a skill-id catalog probe -- see
    /// [`crate::analysis::condition_catalog`], which MCONDCAT already
    /// calibrated against that same catalog), `is_life_leech` its
    /// `x.IsLifeLeech`.
    ///
    /// Note the three are NOT mutually exclusive in GW2EI's own model and
    /// aren't treated as such here: a `buff == 1` hit can be neither
    /// condition- nor leech-classified (`hit_stats`'s documented "fourth
    /// bucket"), and `Power` deliberately keeps it (the filter only removes
    /// condition-based rows).
    pub fn keeps(self, is_strike: bool, is_condition: bool, is_life_leech: bool) -> bool {
        match self {
            DamageType::All => true,
            DamageType::Power => !is_condition,
            DamageType::Strike => is_strike,
            DamageType::Condition => is_condition,
            DamageType::LifeLeech => is_life_leech,
            DamageType::StrikeAndCondition => is_strike || is_condition,
            DamageType::ConditionAndLifeLeech => is_condition || is_life_leech,
            DamageType::StrikeAndLifeLeech => is_strike || is_life_leech,
            DamageType::StrikeAndConditionAndLifeLeech => is_strike || is_condition || is_life_leech,
        }
    }
}

/// GW2EI `DamageSource`
/// (`GW2EIEvtcParser/EIData/DamageModifiers/DamageModifiersUtils.cs:10`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageSource {
    /// Actor + minions (`OutgoingDamageModifier.cs:145`,
    /// `GetHitDamageEvents`).
    All,
    /// Actor only (`SingleActor.cs:781`, `GetJustActorHitDamageEvents` ->
    /// `.Where(x => x.From.Is(AgentItem))`).
    NoPets,
    /// Minions only (`SingleActor.cs:797`,
    /// `GetJustMinionsHitDamageEvents`).
    PetsOnly,
    /// Damage TAKEN. GW2EI has no minion dimension on the incoming side at
    /// all (`IncomingDamageModifier.cs:17-42`).
    Incoming,
}

/// GW2EI's `GainComputer` hierarchy
/// (`GW2EIEvtcParser/EIData/DamageModifiers/GainComputers/`).
///
/// `gain_per_stack` is a PERCENT (`5.0` == 5%), and every multiplier variant
/// returns `g / (100 + g)` for a total percent bonus `g` -- the fraction of
/// the ALREADY-BOOSTED observed damage that the modifier is responsible
/// for. This is the single most important thing to get right: the raw
/// damage in the log already includes the bonus, so the attributable gain
/// is `observed * g/(100+g)`, never `observed * g/100`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GainComputer {
    /// `ByPresence.cs:10-13`: `stack > 0 ? g/(100+g) : 0`.
    ByPresence,
    /// `ByStack.cs:10-13`: `g*stack / (100 + stack*g)`.
    ByStack,
    /// `ByMultiPresence.cs:3-8` -- literally `: GainComputerByStack`, i.e.
    /// the same formula. It differs only in being paired with a
    /// [`BuffTracker::multi`] tracker, whose "stack" is the NUMBER OF
    /// DISTINCT BUFFS PRESENT rather than a stack count
    /// (`BuffsTrackers/BuffsTrackerMulti.cs:7-15`).
    ByMultiPresence,
    /// `ByAbsence.cs:10-13`: `stack == 0 ? g/(100+g) : 0`.
    ByAbsence,
    /// `ByStackPlusConstant.cs:11-14`: with total `t = g*stack + c`,
    /// `t / (100 + t)`. Note this is NON-ZERO at `stack == 0` whenever
    /// `c != 0`.
    ByStackPlusConstant(f64),
    /// `ByMultiplyingStack.cs:10-14`: compounding rather than additive --
    /// `p = 100*(1 + g/100)^stack - 100`, then `p / (100 + p)`.
    ByMultiplyingStack,
    /// `AtLeastN.cs:12-15`: `stack >= n ? g/(100+g) : 0`.
    AtLeastNStacks(u32),
    /// `AtMostN.cs:12-15`: `stack <= n ? g/(100+g) : 0`.
    AtMostNStacks(u32),
    /// `ExactNumber.cs:12-15`: `stack == n ? g/(100+g) : 0`.
    ExactNStacks(u32),
    /// GW2EI's private `GainComputerBySkill`
    /// (`SkillDamageModifier.cs:9-21`) -- the ONLY non-multiplier gain
    /// computer in the entire GW2EI codebase, and the only reason the JSON
    /// `nonMultiplier` flag is ever true. Its `ComputeGain` THROWS; the
    /// per-hit gain is instead hardcoded to `1.0` by
    /// `SkillDamageModifier.ComputeGain` (`:50-57`), so `damageGain`
    /// accumulates RAW damage ("damage done while under the effect"), not a
    /// delta.
    BySkill,
}

impl GainComputer {
    /// `GainComputer.Multiplier` (`GainComputer.cs:5`) -- true for every
    /// variant except [`GainComputer::BySkill`].
    pub fn is_multiplier(self) -> bool {
        !matches!(self, GainComputer::BySkill)
    }

    /// `GainComputer.SkillBased` (`GainComputer.cs:6`).
    pub fn is_skill_based(self) -> bool {
        matches!(self, GainComputer::BySkill)
    }

    /// `ComputeGain(gainPerStack, stack)`.
    ///
    /// [`GainComputer::BySkill`] returns `0.0` rather than panicking (GW2EI
    /// throws, because the code path is unreachable there: its
    /// `SkillDamageModifier` overrides `ComputeGain` wholesale). The engine
    /// never routes a skill-based definition through here either -- see
    /// [`Trigger::Skill`].
    pub fn compute_gain(self, gain_per_stack: f64, stack: u32) -> f64 {
        let g = gain_per_stack;
        let stack_f = f64::from(stack);
        let by_presence = || g / (100.0 + g);
        match self {
            GainComputer::ByPresence => if stack > 0 { by_presence() } else { 0.0 },
            GainComputer::ByStack | GainComputer::ByMultiPresence => {
                let t = g * stack_f;
                t / (100.0 + t)
            }
            GainComputer::ByAbsence => if stack == 0 { by_presence() } else { 0.0 },
            GainComputer::ByStackPlusConstant(c) => {
                let t = g * stack_f + c;
                t / (100.0 + t)
            }
            GainComputer::ByMultiplyingStack => {
                let p = 100.0 * (1.0 + g / 100.0).powf(stack_f) - 100.0;
                p / (100.0 + p)
            }
            GainComputer::AtLeastNStacks(n) => if stack >= n { by_presence() } else { 0.0 },
            GainComputer::AtMostNStacks(n) => if stack <= n { by_presence() } else { 0.0 },
            GainComputer::ExactNStacks(n) => if stack == n { by_presence() } else { 0.0 },
            GainComputer::BySkill => 0.0,
        }
    }
}

/// One entry of GW2EI's `_dlCheckers` list -- a `DamageLogChecker`
/// (`DamageModifiersUtils.cs:12`), i.e. a per-hit predicate over flags that
/// live on the damage row itself.
///
/// GW2EI's checkers are arbitrary C# lambdas; this is a closed enum of the
/// ones expressible from what [`crate::evtc::RawEvent`] already decodes.
/// **All checks on a definition are ANDed** -- `DamageModifierDescriptor.
/// CheckCondition` is `_dlCheckers.All(...)` (`:128-130`). (Its sibling
/// `_earlyExitCheckers` is instead ORed and aborts the whole modifier for
/// that actor, `:133-135`; no definition in this project needs one yet, so
/// it is deliberately not modelled -- Task 2 adds it if the catalog demands
/// it.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitCheck {
    /// `HealthDamageEvent.IsMoving` -- the SOURCE is moving.
    /// `SkillEvent.cs:38`: `IsMoving = (evtcItem.IsMoving & 1) > 0`.
    SrcMoving,
    /// `HealthDamageEvent.AgainstMoving` -- the TARGET is moving.
    /// `SkillEvent.cs:39`: `AgainstMoving = (evtcItem.IsMoving & 2) > 0`.
    AgainstMoving,
    /// `HealthDamageEvent.IsOverNinety` -- the SOURCE is above 90% health
    /// (see [`crate::evtc::RawEvent::is_ninety`]'s doc comment for the
    /// "source's own health, not the target's" nuance).
    OverNinety,
    /// `HealthDamageEvent.AgainstDowned`.
    AgainstDowned,
    /// `HealthDamageEvent.HasCrit`.
    Crit,
    /// `HealthDamageEvent.HasGlanced`.
    Glance,
    /// `dl.SkillID == id`.
    SkillId(u32),
}

/// GW2EI's `BuffsTracker` (`DamageModifiers/BuffsTrackers/`) -- which buff
/// ids a buff-gated modifier watches, and how their stacks are counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuffTracker {
    /// The watched buff (skill) ids.
    pub ids: &'static [u32],
    /// `false` => `BuffsTrackerSingle` (`BuffsTrackerSingle.cs:12-15`):
    /// exactly one id, and the "stack" is that buff's own stack count at
    /// hit time.
    ///
    /// `true` => `BuffsTrackerMulti` (`BuffsTrackerMulti.cs:7-15`): the
    /// "stack" is the NUMBER OF DISTINCT WATCHED BUFFS PRESENT
    /// (`stack += bgms[key].GetStackCount(time) > 0 ? 1 : 0`), never a sum
    /// of stacks.
    pub multi: bool,
    /// Whether the watched buff is an intensity (stacking) buff, for the
    /// [`crate::analysis::buffs::simulator`] run that produces its
    /// stack-count timeline. GW2EI reads this off its own `Buff` catalog
    /// (`BuffStackType`); this project has no full buff catalog yet
    /// (`BOON_IDS` covers only the 12 boons), so a definition declares it.
    pub intensity: bool,
}

/// What makes a hit "qualify" -- GW2EI's `DamageModifierDescriptor`
/// subclass, collapsed to a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// `DamageLogDamageModifier` (`DamageModifierDescriptors/
    /// DamageLogDamageModifier.cs`): no buff state is consulted at all. The
    /// gain is computed ONCE for the whole log at `stack = 1` (`:15-18`),
    /// and the hit qualifies iff every [`DamageModifierDef::checks`] entry
    /// passes (`:21-41`). Its ctor hardcodes
    /// [`GainComputer::ByPresence`] (`:10`).
    Hit,
    /// `BuffOnActorDamageModifier` -- a buff on the DAMAGE DEALER. Stack
    /// count at hit time comes from the actor's buff timeline
    /// (`BuffOnActorDamageModifier.cs:57-61`).
    BuffOnActor {
        tracker: BuffTracker,
        /// `WithBuffOnActorFromFoe()` (`:42`) -- only count the buff when
        /// it was applied BY the hit's foe. Not modelled by the engine yet
        /// (this project's buff extraction records the applier, but no
        /// per-applier stack simulation exists); a definition setting it is
        /// rejected by the engine rather than silently mis-evaluated.
        from_foe: bool,
    },
    /// `BuffOnFoeDamageModifier` -- a buff on the TARGET
    /// (`BuffOnFoeDamageModifier.cs:93-178`), optionally gated by a second
    /// buff condition on the actor (`CheckActor`, `:78-80`).
    ///
    /// GW2EI drops every foe-buff modifier outright in WvW and sPvP
    /// (`BuffOnFoeDamageModifier.Keep`, `:83-91`), which is exactly this
    /// project's primary mode -- so this arm exists for model completeness
    /// and PvE-mode use, and [`DamageModifierDef::keep`] reproduces that
    /// unconditional rejection.
    BuffOnFoe {
        tracker: BuffTracker,
        /// `UsingActorCheckerByPresence`/`ByAbsence` (`:27-51`).
        actor_check: Option<ActorBuffCheck>,
    },
    /// `SkillDamageModifier` (`SkillDamageModifier.cs:32-57`): qualifies iff
    /// `dl.SkillID == id`, with a hardcoded gain of `1.0` (so `damageGain`
    /// is raw damage) and `nonMultiplier == true`.
    Skill(u32),
}

/// `BuffOnFoeDamageModifier`'s secondary condition on the ACTOR
/// (`:9-10, 27-51, 78-80`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorBuffCheck {
    pub tracker: BuffTracker,
    /// `true` => `UsingActorCheckerByAbsence` (gain computer
    /// [`GainComputer::ByAbsence`]), `false` => `...ByPresence`. GW2EI
    /// evaluates it as `ComputeGain(1.0, stack) > 0.0` -- only the sign
    /// matters, `1.0` is a dummy `gainPerStack`.
    pub by_absence: bool,
}

/// GW2EI `DamageModifierMode` (`DamageModifiersUtils.cs:9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierMode {
    PvE,
    PvEInstanceOnly,
    SPvP,
    WvW,
    All,
    SPvPWvW,
    PvEWvW,
    PvESPvP,
}

/// GW2EI `ParseModeEnum` (`GW2EIEvtcParser/LogLogic/LogLogic.cs:25`),
/// narrowed to the distinctions [`DamageModifierDef::keep`] actually makes:
/// every `Instanced*`/`Benchmark` value behaves identically there, so they
/// collapse into [`ParseMode::Instanced`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseMode {
    Instanced,
    WvW,
    SPvP,
    OpenWorld,
    Unknown,
}

/// GW2EI `SkillModeEnum` (`LogLogic.cs:26`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillMode {
    PvE,
    WvW,
    SPvP,
}

/// Where a modifier comes from -- GW2EI's `Source` enum, kept as an opaque
/// label because nothing in the engine branches on it (GW2EI only uses it
/// to decide WHICH modifiers apply to a given actor's spec, via
/// `DamageModifiersContainer.GetPersonalOutgoingModifiersPerSpec`). Task 2's
/// catalog fills in per-spec sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSource {
    /// Consumables / runes / sigils (`ItemDamageModifiers.cs`).
    Item,
    /// Gear-stat modifiers (`GearDamageModifiers.cs`).
    Gear,
    /// Profession-agnostic shared modifiers (`SharedDamageModifiers.cs`).
    Common,
    /// Encounter-specific (`EncounterDamageModifiers.cs`).
    Encounter,
    /// A profession/elite-spec modifier; the string is the spec name as
    /// this project spells it in `Player::elite_spec`/`profession`.
    Spec(&'static str),
}

/// One damage-modifier definition.
///
/// `id` is always the POSITIVE GW2EI id (`DamageModifierIDs.cs`, e.g.
/// `Mod_MovingBonus = 10`); the JSON-facing signed id is
/// [`DamageModifierDef::json_id`], which negates it for incoming modifiers
/// (`DamageModifier.cs:26`, `ID => Incoming ? -descriptor.ID : descriptor.ID`;
/// the `"d"` prefix in `damageModMap` is added at serialization time,
/// `GW2EIBuilders/JsonModels/JsonLogBuilder.cs:326-330`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageModifierDef {
    /// Strictly positive; GW2EI's ctor throws otherwise
    /// (`DamageModifierDescriptor.cs:51`).
    pub id: i32,
    pub name: &'static str,
    pub icon: &'static str,
    /// GW2EI's `InitialTooltip` -- the bare description, WITHOUT the
    /// `"<br>No Minions"` / `"<br>Applied on ..."` / `"<br>Compared against
    /// ..."` / `"<br>Counter"` / `"<br>Non multiplier"` / `"<br>Approximate"`
    /// suffixes that `DamageModifier`'s ctor appends
    /// (`DamageModifier.cs:35-68`). Those are derivable from the other
    /// fields, so they are composed at emission time (Task 3) rather than
    /// baked into the catalog.
    pub description: &'static str,
    pub source: ModSource,
    /// PERCENT per stack; GW2EI's ctor throws on `0.0`
    /// (`DamageModifierDescriptor.cs:60`). Negative for damage REDUCTION
    /// modifiers (which the same formulas handle: `-10/(100-10)`).
    pub gain_per_stack: f64,
    pub gain: GainComputer,
    pub trigger: Trigger,
    /// Selects the eligible hits (`totalHitCount`'s pool).
    pub src_type: DamageType,
    /// Selects the DENOMINATOR aggregate (`totalDamage`). Frequently
    /// different from `src_type` -- see this module's `mod.rs` writeup.
    pub compare_type: DamageType,
    pub dmg_src: DamageSource,
    /// ANDed per-hit predicates (`CheckCondition`, `:128-130`).
    pub checks: &'static [HitCheck],
    pub mode: ModifierMode,
    /// `.UsingApproximate()` (`:184`). Approximate modifiers are dropped in
    /// WvW/sPvP entirely (`Keep`, `:156`).
    pub approximate: bool,
    /// `IsCounter` (`CounterOn{Actor,Foe}DamageModifier.cs:9`): the gain is
    /// forced to `1.0` per qualifying hit (`:21-25`), so `damageGain` is
    /// raw "damage during" and the headline number is
    /// `hitCount / totalHitCount`. Also forces `Multiplier == true`
    /// (`DamageModifierDescriptor.cs:21`).
    pub is_counter: bool,
    /// `.WithBuilds(min, max)` (`:74`) -- a HALF-OPEN `[min, max)` GW2 build
    /// interval (`Available`, `:138-150`). This is the "era" mechanism: a
    /// reworked trait becomes two definitions with different ids and
    /// disjoint build ranges.
    pub min_gw2_build: u64,
    pub max_gw2_build: u64,
    /// `.WithEvtcBuilds(min, max)` (`:81`) -- same half-open convention over
    /// the arcdps/evtc build.
    pub min_evtc_build: i64,
    pub max_evtc_build: i64,
}

/// GW2EI `GW2Builds.StartOfLife` / `ArcDPSBuilds.StartOfLife` stand-in.
pub const START_OF_LIFE: u64 = 0;
/// GW2EI `GW2Builds.EndOfLife` stand-in.
pub const END_OF_LIFE: u64 = u64::MAX;
/// GW2EI `ArcDPSBuilds.StartOfLife` stand-in.
pub const EVTC_START_OF_LIFE: i64 = i64::MIN;
/// GW2EI `ArcDPSBuilds.EndOfLife` stand-in.
pub const EVTC_END_OF_LIFE: i64 = i64::MAX;

impl DamageModifierDef {
    /// `DamageModifierDescriptor.Multiplier` (`:21`): the gain computer's
    /// own flag, ORed with `IsCounter`.
    pub fn is_multiplier(&self) -> bool {
        self.gain.is_multiplier() || self.is_counter
    }

    /// The JSON `nonMultiplier` field
    /// (`JsonLogBuilder.cs:108-122`: `NonMultiplier = !Multiplier`).
    pub fn non_multiplier(&self) -> bool {
        !self.is_multiplier()
    }

    /// The JSON `skillBased` field (`DamageModifierDescriptor.cs:22`).
    pub fn skill_based(&self) -> bool {
        self.gain.is_skill_based()
    }

    /// `DamageModifier.Incoming` -- implied by
    /// [`DamageSource::Incoming`] (both GW2EI ctors assert the equivalence).
    pub fn incoming(&self) -> bool {
        self.dmg_src == DamageSource::Incoming
    }

    /// The signed id used as the `damageModMap` key (minus the `"d"`
    /// prefix): `DamageModifier.cs:26`.
    pub fn json_id(&self) -> i32 {
        if self.incoming() { -self.id } else { self.id }
    }

    /// `DamageModifierDescriptor.Available` (`:138-150`): both build
    /// windows are HALF-OPEN, `build >= min && build < max`.
    ///
    /// `None` means "the log doesn't carry that build stamp". GW2EI always
    /// has both (it synthesizes zero-valued events when absent, and
    /// `0 >= StartOfLife && 0 < EndOfLife` holds, so an unknown build keeps
    /// every un-gated definition). This mirrors that: an absent build only
    /// fails a definition that actually narrowed the corresponding window.
    pub fn available(&self, gw2_build: Option<u64>, evtc_build: Option<i64>) -> bool {
        let gw2 = gw2_build.unwrap_or(START_OF_LIFE);
        let evtc = evtc_build.unwrap_or(EVTC_START_OF_LIFE);
        gw2 >= self.min_gw2_build
            && gw2 < self.max_gw2_build
            && evtc >= self.min_evtc_build
            && evtc < self.max_evtc_build
    }

    /// `DamageModifierDescriptor.Keep` (`:152-180`), plus
    /// `BuffOnFoeDamageModifier.Keep`'s override (`:83-91`).
    ///
    /// Statement order matters and is preserved: the approximate/PvP
    /// rejection comes BEFORE the `Mode == All` short-circuit, so an
    /// `All`-mode approximate modifier is still dropped in WvW/sPvP.
    pub fn keep(&self, parse_mode: ParseMode, skill_mode: SkillMode) -> bool {
        // `BuffOnFoeDamageModifier.Keep:86` -- unconditional, ahead of
        // everything else (target buff tracking is unreliable in PvP).
        if matches!(self.trigger, Trigger::BuffOnFoe { .. })
            && matches!(parse_mode, ParseMode::WvW | ParseMode::SPvP)
        {
            return false;
        }
        // `:156`
        if self.approximate && matches!(parse_mode, ParseMode::WvW | ParseMode::SPvP) {
            return false;
        }
        // `:162`
        if self.mode == ModifierMode::All {
            return true;
        }
        match skill_mode {
            SkillMode::PvE => {
                if matches!(parse_mode, ParseMode::OpenWorld | ParseMode::Unknown) {
                    // `:171` -- note `PvEInstanceOnly` is excluded here.
                    !self.approximate
                        && matches!(
                            self.mode,
                            ModifierMode::PvE | ModifierMode::PvEWvW | ModifierMode::PvESPvP
                        )
                } else {
                    // `:173`
                    matches!(
                        self.mode,
                        ModifierMode::PvE
                            | ModifierMode::PvEInstanceOnly
                            | ModifierMode::PvEWvW
                            | ModifierMode::PvESPvP
                    )
                }
            }
            // `:175`
            SkillMode::WvW => matches!(
                self.mode,
                ModifierMode::WvW | ModifierMode::SPvPWvW | ModifierMode::PvEWvW
            ),
            // `:177`
            SkillMode::SPvP => matches!(
                self.mode,
                ModifierMode::SPvP | ModifierMode::SPvPWvW | ModifierMode::PvESPvP
            ),
        }
    }
}
