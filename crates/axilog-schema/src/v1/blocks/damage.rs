use super::ByEntity;
use crate::v1::catalogs::CatalogBuilder;
use crate::v1::entities::{EntityIndex, Role};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageBlock {
    pub squad: DamageSquad,
    pub by_entity: ByEntity<DamageEntity>,
}

impl DamageBlock {
    /// Whether this block carries nothing at all, which is what
    /// [`crate::v1::envelope::CoverageState::Empty`] reports. `squad` is a
    /// sum over the `by_entity` rows, so no rows implies a zero aggregate
    /// and there is nothing else to check.
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Aggregates `Role::Squad` entities ONLY -- not every friendly player.
/// `DamageBlock::by_entity` is the full roster (squad AND non-squad
/// friendlies); this aggregate deliberately excludes non-squad friendlies
/// (`Role::FriendlyPlayer`) so its name stays true even once the upstream
/// `Player::in_squad` gap (currently hardcoded `true` for every friendly)
/// is filled and non-squad friendlies start actually appearing.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageSquad {
    pub total: u64,
    pub dps: f64,
}

#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct DamageEntity {
    pub total: u64,
    pub dps: f64,
    pub taken: u64,
    /// Enemy players this entity landed the DOWNING blow on -- the legacy
    /// `PlayerOut::downs_dealt`. An outgoing OUTCOME, so it lives here
    /// beside the damage that produced it, not on `defenses` (which carries
    /// the incoming mirror, `downs_taken`/`deaths`) -- the same split GW2EI
    /// makes, whose `defenses[0]` carries `downCount`/`deadCount` while the
    /// offensive per-target rows carry `downed`/`killed`. Always present:
    /// the legacy field is ungated.
    pub downs_dealt: u32,
    /// Enemy players this entity landed the KILLING blow on -- the legacy
    /// `PlayerOut::kills_dealt`. See `downs_dealt`.
    pub kills_dealt: u32,
    /// Breakbar damage this entity DEALT -- the legacy
    /// `PlayerOut::breakbar_damage_dealt`, which had no 1.0 destination
    /// and is the one `players[]` field the ei-json adapter could not read
    /// from this document (it feeds `dpsAll[0].breakbarDamage`).
    ///
    /// Here rather than on `defenses` for the reason `downs_dealt` gives
    /// above: this is outgoing. `DefensesEntity::breakbar_count`/
    /// `breakbar_damage` are its INCOMING mirror, and the legacy field's
    /// origin (`Metrics::defenses`) is where it was computed, not what it
    /// measures.
    pub breakbar_damage_dealt: u64,
    /// Keyed by the TARGET's entity id -- so it joins directly to that
    /// entity's own row. Sparse; omitted when empty.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub per_target: BTreeMap<u32, PerTarget>,
    /// OUTGOING per-skill damage, keyed by skill id. `Some` exactly when
    /// the per-skill compute gate (`--skill-damage` / SDK
    /// `skill_damage: true`) was on, `None` otherwise.
    ///
    /// **This `Option` is the format's `--skill-damage` GATE RECORD, and
    /// `Some({})` is a meaningful value.** Side-channel absorption Task 7
    /// found that an empty map cannot distinguish "the pass never ran" from
    /// "this entity landed nothing", which left the ei-json adapter reading
    /// the legacy `PlayerOut::skill_damage`'s presence -- private data no
    /// consumer of this document could see. An entity that dealt no damage
    /// under a gate that WAS on gets `Some({})`; one whose gate was off
    /// gets no key at all. `coverage.damage` cannot answer this, because
    /// this block's other halves are always-on: `damage` is the third
    /// two-gate block, after `replay` and `boons`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_skill: Option<BTreeMap<u32, SkillRow>>,
    /// INCOMING per-skill damage, keyed by skill id -- the legacy
    /// `SkillDamageOut::taken`, which had no 1.0 destination at all before
    /// the final review. Named to mirror this row's own `total`/`taken`
    /// pair, so the outgoing/incoming split reads the same way at both
    /// levels. Same gate as `by_skill`, and the same `Option` reading of
    /// it; `sum(by_skill_taken[*].total) == taken` holds by construction
    /// (see `SkillDamageOut`'s doc comment).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_skill_taken: Option<BTreeMap<u32, SkillRow>>,
}

/// One `(entity, target)` pair.
///
/// `total` is ungated (the legacy `DamageOut::per_enemy`). Everything else
/// here comes from the `--skill-damage`-gated families (`PlayerOut::
/// per_target` and `SkillDamageOut::per_target`), so a row can legitimately
/// carry `total` alone.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct PerTarget {
    pub total: u64,
    /// The gated per-target offensive detail -- the legacy
    /// `PerTargetStatsOut`, which had no 1.0 destination before the final
    /// review.
    ///
    /// Grouped under ONE optional key rather than flattened onto
    /// `PerTarget`, deliberately: the 23 fields of `PerTargetDetail` are
    /// computed only when `--skill-damage` is on, so flattening them would
    /// force this row to publish 23 fabricated zeros whenever the gate is
    /// off -- exactly the "absent reported as zero" ambiguity `coverage`
    /// exists to remove, one level down. One `Option` gives that gate a
    /// single, unambiguous presence signal.
    ///
    /// The gate itself is unchanged by Phase B's widening: the underlying
    /// pass (`PlayerMetrics::per_target`) is unconditional -- one shared
    /// scan computed regardless of the flag -- so `--skill-damage` is a
    /// SERIALIZATION gate only, same as before. It stays a gate at all
    /// because always-on was measured at +56.5% on the rendered HTML report
    /// with the original 8 fields per pair (`crate::PerTargetStatsOut`'s
    /// doc comment has the numbers); 23 fields is a larger payload for the
    /// same shape, not a smaller one, so the gate's justification only
    /// strengthens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<PerTargetDetail>,
    /// Per-`(entity, target, skill)` outgoing damage, keyed by skill id --
    /// the legacy `SkillDamageOut::per_target`, which had no 1.0
    /// destination before the final review. Same gate as
    /// `DamageEntity::by_skill`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub by_skill: BTreeMap<u32, SkillRow>,
}

/// Mirrors the legacy `crate::PerTargetStatsOut` field-for-field, minus
/// `enemy_id` -- that is the enclosing map's KEY here (as the target's
/// ENTITY id, joined through `EntityIndex::by_enemy_id`), not carried
/// redundantly inside the value. Same convention `series::TargetSeries`
/// already uses.
///
/// `downed`/`killed` are this pair's split of `DamageEntity::downs_dealt`/
/// `kills_dealt`; `interrupts` and `downs_contribution_damage` are NOT
/// recoverable from any other block, which is what made dropping this
/// struct a real data loss rather than a redundancy.
///
/// 23 fields total (Phase B widened this from 7): the 22 of
/// `axilog_core::analysis::per_target::PerTargetOffense`, plus
/// `downs_contribution_damage` above, which comes from `PlayerMetrics`
/// rather than that struct -- see its own doc comment.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct PerTargetDetail {
    pub connected_hits: u32,
    pub connected_damage: u64,
    pub against_downed_count: u32,
    pub downed: u32,
    pub killed: u32,
    pub interrupts: u32,
    /// arcdps-methodology down-contribution DAMAGE credited to this entity
    /// for downs of this specific target -- NOT GW2EI's own
    /// 90%-to-downstate-window algorithm. See `crate::PerTargetStatsOut`.
    pub downs_contribution_damage: u64,
    /// EI's `directDmg` COUNT pair, mirroring `PerTargetOffense::direct_count`.
    pub direct_count: u32,
    /// EI's `directDmg` -- the damage sum over `is_direct_hit` rows against
    /// this one target. Deliberately NOT the same quantity as this crate's
    /// `connected_direct_dmg` (a whole-fight figure derived differently);
    /// collapsing the two would silently swap in the wrong number under a
    /// plausible-looking name. See `crate::PerTargetStatsOut::direct_damage`.
    pub direct_damage: u64,
    /// EI's `criticalRate` numerator for this target.
    pub crit_count: u32,
    /// EI's `criticalDmg` for this target.
    pub crit_damage: u64,
    /// EI's `flankingRate` numerator for this target.
    pub flank_count: u32,
    /// EI's `glanceRate` numerator for this target.
    pub glance_count: u32,
    /// EI's `criticalRate` DENOMINATOR for this target -- NOT `direct_count`
    /// above. Native-only: EI never publishes a per-target crit-rate
    /// denominator, so there is no ei-json key for this field.
    pub critable_direct_count: u32,
    /// EI's `againstDownedDamage` -- the damage pair for
    /// `against_downed_count` above, scoped to this one target.
    pub against_downed_damage: u64,
    /// EI's `missed` against this target -- arcdps `BLIND`.
    pub missed: u32,
    /// EI's `evaded` against this target -- arcdps `EVADE`.
    pub evaded: u32,
    /// EI's `blocked` against this target -- arcdps `BLOCK`.
    pub blocked: u32,
    /// EI's `invulned` against this target -- arcdps `ABSORB`/`INVERT`.
    pub invulned: u32,
    /// EI's `appliedCrowdControl` against this target.
    pub applied_total: u32,
    /// EI's `appliedCrowdControlDuration`, ms.
    pub applied_duration_ms: u64,
    /// EI's `appliedCrowdControlDownContribution` against this target.
    pub applied_downs_contribution: u32,
    /// EI's `appliedCrowdControlDurationDownContribution`, ms.
    pub applied_duration_downs_contribution_ms: u64,
}

/// Mirrors `crate::SkillEntryOut` field-for-field. `min`/`max` are `u64`
/// there (not `Option<u32>` as an earlier draft of this block assumed --
/// the legacy struct always populates them, no optionality to preserve),
/// so this row keeps them non-optional too rather than inventing an
/// `Option` the source data never has.
#[derive(Serialize, Debug, Default, Clone, PartialEq)]
pub struct SkillRow {
    pub total: u64,
    /// CONTRIBUTING (`dmg > 0`) row count -- see `crate::SkillEntryOut::
    /// hits` for the predicate and its documented divergence from GW2EI's
    /// own attempt-count `hits`.
    ///
    /// `Option` because the ENEMY rows this block also carries (side-channel
    /// absorption Task 7) come from a different pass,
    /// `skill_damage::build_enemy_dist`, which counts `HasHit` rows instead
    /// -- a superset that includes zero-damage connecting hits. That pass
    /// never computes the contributing count at all, so an enemy row omits
    /// this field rather than publishing a `0` that a consumer would divide
    /// `total` by. Player rows are always `Some`, so the serialized bytes
    /// are unchanged for them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hits: Option<u32>,
    /// `HasHit` row count -- GW2EI's `connectedHits`
    /// (`JsonDamageDistBuilder.cs:72`, `dmgEvt.HasHit ? 1 : 0`), which
    /// counts a connecting hit that dealt zero health damage and excludes
    /// the blocked/evaded/missed/invulned cases.
    ///
    /// Populated for ENEMY rows by Task 7. Task 9 fills it for player rows
    /// from `dist_outcomes`, a third pass that measures the same quantity on
    /// the friendly side -- which is why this is a distinct field from
    /// `hits` rather than a role-dependent reinterpretation of it. The
    /// ei-json adapter emits the two under distinct keys already (`hits` vs
    /// `connectedHits`), and axibridge's mitigation math divides by
    /// `connectedHits` specifically.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_hits: Option<u32>,
    pub min: u64,
    pub max: u64,
    /// Hit COUNT (not a damage sum) of hits that crit, mirroring
    /// `crate::SkillEntryOut::crit_hits` -- see that field's doc comment
    /// for the derivation. Fix round 2, Finding: this was dropped entirely
    /// in the first pass of this block, with no stated reason -- a real
    /// equivalence gap, not a design choice.
    pub crit_hits: u32,
    /// Hit COUNT of hits landed while flanking, mirroring
    /// `crate::SkillEntryOut::flank_hits`. Same fix-round-2 correction as
    /// `crit_hits` above.
    pub flank_hits: u32,
    /// Of `total`, the portion dealt by a player or a player's minion --
    /// siege, guards, NPCs and unattributable rows are the remainder. Lets
    /// a consumer show incoming damage the way the arcdps in-game filters
    /// do, without a second distribution.
    ///
    /// Present on `by_skill_taken` PLAYER rows only: that is the one
    /// grouping where the question is meaningful and the one pass that
    /// classifies sources. On `by_skill`/`per_target` it is absent because
    /// those rows are squad-player-sourced by construction, and on enemy
    /// rows because that pass does not measure it -- absent is "not
    /// measured", never "no player damage", the same convention as `hits`
    /// and `connected_hits` above.
    ///
    /// `player_total <= total` holds per row by construction; the split is
    /// a refinement of `total`, not a filter on it, so
    /// `sum(by_skill_taken[*].total) == taken` is unaffected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_total: Option<u64>,
    /// The hit-OUTCOME breakdown for this row, from
    /// `axilog_core::analysis::dist_outcomes` (side-channel absorption
    /// Task 9). See [`SkillOutcomeCols`] for why these eight live in a
    /// nested struct rather than as eight sibling `Option`s.
    ///
    /// Present on PLAYER rows (both `by_skill` and `by_skill_taken`) when
    /// `--skill-damage` is on, absent on enemy rows -- that pass only runs
    /// over the friendly side. Absent is therefore "this pass did not
    /// measure this row", never "every attempt connected".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcomes: Option<SkillOutcomeCols>,
}

/// The outcome columns a `dist_outcomes` row contributes to a [`SkillRow`],
/// beyond the `connected_hits` it shares with the enemy pass.
///
/// Nested behind one `Option` rather than spread across the parent as eight
/// `Option` fields because their presence is genuinely all-or-nothing: they
/// come from a single pass over a single event list, so no combination of
/// "this counter measured, that one didn't" can arise. One `Option` states
/// that invariant in the type instead of leaving eight fields free to
/// disagree, and it gives a consumer a single presence check for "were
/// outcomes computed at all" -- which is exactly the branch the ei-json
/// adapter takes, emitting all eight keys together or none.
///
/// `connected_hits` is deliberately NOT here: it stays on [`SkillRow`]
/// because the ENEMY pass measures that same quantity too (Task 7), and
/// duplicating it would leave two fields for one number with nothing
/// forcing them to agree.
#[derive(Serialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct SkillOutcomeCols {
    /// GW2EI's own `hits`: every row that is not one of its
    /// `NoDamageHealthDamageEvent` markers -- an ATTEMPT count, and a third
    /// distinct quantity from [`SkillRow::hits`] (contributing rows, `dmg >
    /// 0`) and [`SkillRow::connected_hits`] (`HasHit` rows). All three are
    /// separate fields for the reason Task 7 gave: one field reinterpreted
    /// by context is how a consumer divides by the wrong denominator.
    pub attempt_hits: u32,
    /// Zero on a condition skill: GW2EI zeroes this and the four below
    /// inside its `if (!IndirectDamage)` guard, and `dist_outcomes`
    /// reproduces that in its own post-pass.
    pub glance: u32,
    pub missed: u32,
    pub evaded: u32,
    pub blocked: u32,
    /// NOT inside the indirect guard above -- a condition tick can land on
    /// an invulnerable target and GW2EI counts it.
    pub invulned: u32,
    pub interrupted: u32,
    /// GW2EI's per-skill `IndirectDamage` flag: this skill produced at
    /// least one non-direct (condition) damage row. Every strike-damage
    /// surface downstream uses it as a skip filter, so without it condition
    /// ticks read as strike damage.
    pub indirect: bool,
}

/// One legacy `SkillEntryOut` as a [`SkillRow`]. Shared by the three
/// per-skill families this block carries (`by_skill`, `by_skill_taken`, and
/// each `PerTarget::by_skill`) so a field added to one cannot silently miss
/// the other two -- which is how `crit_hits`/`flank_hits` were dropped once
/// already.
fn skill_row(e: &crate::SkillEntryOut) -> SkillRow {
    SkillRow {
        total: e.total,
        hits: Some(e.hits),
        // Not measured on the friendly side by this pass; Task 9's
        // `dist_outcomes` fills it.
        connected_hits: None,
        min: e.min,
        max: e.max,
        crit_hits: e.crit_hits,
        flank_hits: e.flank_hits,
        player_total: e.player_total,
        // Task 9's `merge_outcomes` fills this in a second pass over the
        // assembled map, because its row set is a superset of this one's.
        outcomes: None,
    }
}

/// One `skill_damage::SkillEntry` from the enemy pass as a [`SkillRow`].
///
/// Deliberately NOT [`skill_row`]: that helper's input counts contributing
/// rows and this one counts `HasHit` rows, so the same `hits` number means
/// two different things and lands in two different fields. Sharing the
/// helper would silently equate them.
fn enemy_skill_row(e: &axilog_core::analysis::skill_damage::SkillEntry) -> SkillRow {
    SkillRow {
        total: e.total,
        hits: None,
        connected_hits: Some(e.hits),
        min: e.min,
        max: e.max,
        crit_hits: e.crit_hits,
        flank_hits: e.flank_hits,
        // Enemy OUTGOING rows: the source is the enemy, so the split would
        // be a tautology even if this pass classified sources (it does not).
        player_total: None,
        // The outcome pass runs over the friendly side only.
        outcomes: None,
    }
}

/// Fold one player-side distribution's outcome rows into the [`SkillRow`]
/// map `skill_damage` already built for it.
///
/// **This is a UNION, not an enrichment** -- which is the one place Task 9
/// departs from its plan sketch. The two passes disagree about which skills
/// exist, on purpose: `skill_damage` accumulates only CONTRIBUTING (`dmg >
/// 0`) rows, so a skill whose every attempt was blocked never reaches it,
/// while `dist_outcomes` counts exactly those rows and GW2EI emits them
/// (`totalDamage: 0, hits: n`). Those pure-mitigation rows are the reason
/// the outcome pass exists at all, so asserting the row sets match -- as
/// the plan proposed -- would have failed on the first real log, and
/// intersecting them would have dropped the payload. The ei-json adapter
/// has emitted this same union since MEIGAP2; absorbing it here just moves
/// the union one layer down, to the container both readers share.
///
/// An outcome-only row gets `hits: Some(0)`, not `None`: absence from
/// `skill_damage` is a measurement (zero contributing events), not a gap.
/// That is Task 8's rule -- zero-fill where the number is genuinely known,
/// omit only where the pass never looked.
///
/// `player_split` applies that same rule to [`SkillRow::player_total`]: a
/// row materialized here has `total: 0` (every attempt was blocked, evaded
/// or missed, so no damage was dealt at all), which makes its player-sourced
/// portion genuinely known to be 0 rather than unmeasured -- even though the
/// classifying pass never saw the row. Zero-filling it keeps "present on
/// every `by_skill_taken` row" a flat invariant, so a consumer can feature-
/// detect the split from any row instead of having to find one that happens
/// to carry damage. Only the taken side passes `true`; on `by_skill` the
/// field stays absent for the reason given on [`SkillRow::player_total`].
fn merge_outcomes(
    rows: &mut BTreeMap<u32, SkillRow>,
    outcomes: &[axilog_core::analysis::dist_outcomes::SkillOutcomes],
    cats: &mut CatalogBuilder,
    player_split: bool,
) {
    for o in outcomes {
        cats.reference_skill(o.skill_id);
        let row = rows.entry(o.skill_id).or_insert_with(|| SkillRow {
            hits: Some(0),
            player_total: player_split.then_some(0),
            ..SkillRow::default()
        });
        row.connected_hits = Some(o.connected_hits);
        row.outcomes = Some(SkillOutcomeCols {
            attempt_hits: o.hits,
            glance: o.glance,
            missed: o.missed,
            evaded: o.evaded,
            blocked: o.blocked,
            invulned: o.invulned,
            interrupted: o.interrupted,
            indirect: o.indirect,
        });
    }
}

pub fn build_damage(
    report: &crate::Report,
    index: &EntityIndex,
    cats: &mut CatalogBuilder,
    enemy_dist: Option<&BTreeMap<u64, Vec<axilog_core::analysis::skill_damage::SkillEntry>>>,
    dist_outcomes: Option<&BTreeMap<u64, axilog_core::analysis::dist_outcomes::DistOutcomes>>,
) -> DamageBlock {
    let mut by_entity = ByEntity::default();
    let mut squad_total = 0u64;
    let mut squad_dps = 0f64;

    for p in &report.players {
        let Some(entity_id) = index.by_agent_addr(p.agent_addr) else {
            continue;
        };
        if index.role_of(entity_id) == Some(Role::Squad) {
            squad_total += p.damage.total;
            squad_dps += p.damage.dps;
        }

        // The three per-target families are UNIONED, not zipped: their
        // enemy-id sets genuinely differ. `PlayerOut::per_target` is itself
        // already the union of the per-target offense map and the
        // per-target down-contribution map (see `build_report`), so an
        // enemy can appear there with no `per_enemy` damage row at all --
        // a credit inside that enemy's down window without a landed hit
        // over the whole fight. Keying one map by entity id and filling it
        // from each source in turn keeps every row reachable.
        let mut per_target: BTreeMap<u32, PerTarget> = BTreeMap::new();
        for pe in &p.damage.per_enemy {
            let Some(tid) = index.by_enemy_id(pe.enemy_id) else { continue };
            per_target.entry(tid).or_default().total = pe.total;
        }
        if let Some(stats) = p.per_target.as_ref() {
            for s in stats {
                let Some(tid) = index.by_enemy_id(s.enemy_id) else { continue };
                per_target.entry(tid).or_default().detail = Some(PerTargetDetail {
                    connected_hits: s.connected_hits,
                    connected_damage: s.connected_damage,
                    against_downed_count: s.against_downed_count,
                    downed: s.downed,
                    killed: s.killed,
                    interrupts: s.interrupts,
                    downs_contribution_damage: s.downs_contribution_damage,
                    direct_count: s.direct_count,
                    direct_damage: s.direct_damage,
                    crit_count: s.crit_count,
                    crit_damage: s.crit_damage,
                    flank_count: s.flank_count,
                    glance_count: s.glance_count,
                    critable_direct_count: s.critable_direct_count,
                    against_downed_damage: s.against_downed_damage,
                    missed: s.missed,
                    evaded: s.evaded,
                    blocked: s.blocked,
                    invulned: s.invulned,
                    applied_total: s.applied_total,
                    applied_duration_ms: s.applied_duration_ms,
                    applied_downs_contribution: s.applied_downs_contribution,
                    applied_duration_downs_contribution_ms: s
                        .applied_duration_downs_contribution_ms,
                });
            }
        }

        // `Some` exactly when the gate was on, EVEN IF the maps come out
        // empty -- that is the whole point (see `DamageEntity::by_skill`).
        let mut by_skill = p.skill_damage.as_ref().map(|_| BTreeMap::new());
        let mut by_skill_taken = p.skill_damage.as_ref().map(|_| BTreeMap::new());
        if let (Some(skill_damage), Some(by_skill), Some(by_skill_taken)) =
            (p.skill_damage.as_ref(), by_skill.as_mut(), by_skill_taken.as_mut())
        {
            for e in &skill_damage.outgoing {
                cats.reference_skill(e.skill_id);
                by_skill.insert(e.skill_id, skill_row(e));
            }
            for e in &skill_damage.taken {
                cats.reference_skill(e.skill_id);
                by_skill_taken.insert(e.skill_id, skill_row(e));
            }
            for t in &skill_damage.per_target {
                let Some(tid) = index.by_enemy_id(t.enemy_id) else { continue };
                let row = per_target.entry(tid).or_default();
                for e in &t.skills {
                    cats.reference_skill(e.skill_id);
                    row.by_skill.insert(e.skill_id, skill_row(e));
                }
            }
            // Task 9. Inside the `skill_damage` guard on purpose: both this
            // pass and the distributions above ride the SAME
            // `--skill-damage` request, so an outcome row arriving without
            // a distribution to annotate would mean the caller built the
            // two off different flags. Merging outside the guard would
            // materialize `by_skill` rows for a player whose block is
            // absent, and `by_skill`'s presence is precisely the signal
            // Task 7 taught the adapter to read as "the gate was on".
            //
            // Keyed by the account's representative agent address -- the
            // same `p.agent_addr` the ei-json adapter joined on, not the
            // positional join over `report.players` the plan assumed.
            // `per_target`'s rows get no outcome columns: the pass produces
            // exactly two distributions, whole-fight outgoing and taken,
            // with no per-target split to join to.
            if let Some(o) = dist_outcomes.and_then(|m| m.get(&p.agent_addr)) {
                merge_outcomes(by_skill, &o.outgoing, cats, false);
                merge_outcomes(by_skill_taken, &o.taken, cats, true);
            }
        }

        by_entity.insert(
            entity_id,
            DamageEntity {
                total: p.damage.total,
                dps: p.damage.dps,
                taken: p.damage_taken,
                downs_dealt: p.downs_dealt,
                kills_dealt: p.kills_dealt,
                breakbar_damage_dealt: p.breakbar_damage_dealt,
                per_target,
                by_skill,
                by_skill_taken,
            },
        );
    }

    // Enemy rows. The identity/statistics split's whole point is that an
    // enemy is an entity like any other, so its OUTGOING damage belongs on
    // this block rather than on its `entities[]` identity row -- where the
    // legacy shape had to keep it (`EnemyOut::damage_out`, `#[serde(skip)]`
    // and EI-adapter-only precisely because it had nowhere else to go).
    //
    // `total`/`dps` are always filled; `by_skill` joins the `enemy_dist`
    // pass when `--skill-damage` is on (side-channel absorption Task 7 --
    // it lands in the SAME `by_skill` the player rows use, because the
    // identity/statistics split's whole point is that an enemy is an entity
    // like any other). Every other column stays at its default, which means
    // "not measured for an enemy" -- the same thing an absent row would
    // mean, so nothing reads a zero as a measurement.
    //
    // Iterating `report.enemies` (the combat-participant roster) rather
    // than `ei_targets` (the curated EI one) is deliberate: this is the
    // NATIVE surface, so it follows native's own enemy list. The two
    // filters genuinely differ, but not in a way that can lose a number --
    // criterion (c) of `Metrics::combat_participant_enemies` is "dealt
    // nonzero damage", so every enemy with a nonzero `damage_out` is in
    // `enemies` by construction. An `ei_targets`-only enemy is one that
    // never dealt damage, and its absent row and a zero row say the same
    // thing.
    let secs = (report.encounter.duration_ms as f64 / 1000.0).max(1.0);
    for e in &report.enemies {
        let Some(entity_id) = index.by_enemy_id(e.id) else { continue };
        // The pass is keyed by the enemy's REPRESENTATIVE agent id, which is
        // `Enemy::id` -- the same key `index.by_enemy_id` takes, so this is
        // a direct join rather than a positional one.
        //
        // `Some` whenever the pass RAN, so an enemy it found nothing for
        // reports `Some({})` rather than looking gated-off -- the fill Task
        // 8 introduced for `blocks.series`, applied here to the field that
        // is now this format's `--skill-damage` gate record (see
        // `DamageEntity::by_skill`).
        let by_skill: Option<BTreeMap<u32, SkillRow>> = enemy_dist.map(|d| {
            d.get(&e.id).map_or_else(BTreeMap::new, |skills| {
                skills
                    .iter()
                    .map(|s| {
                        cats.reference_skill(s.skill_id);
                        (s.skill_id, enemy_skill_row(s))
                    })
                    .collect()
            })
        });
        by_entity.insert(
            entity_id,
            DamageEntity {
                total: e.damage_out,
                // Same `total / max(duration_secs, 1)` convention the player
                // rows above carry from `PlayerMetrics::dps`, so the two
                // kinds of row on this block mean the same thing.
                dps: e.damage_out as f64 / secs,
                by_skill,
                ..DamageEntity::default()
            },
        );
    }

    // The two enemy sources are UNIONED, for the same reason the three
    // per-target families above are. `report.enemies` is the
    // combat-participant roster, whose criterion (c) is "dealt nonzero
    // damage"; `enemy_dist` keys off any actor that produced a
    // `HealthDamageEvent`, and `build_enemy_dist` deliberately KEEPS
    // legitimate all-zero rows (a connecting hit that dealt no health
    // damage). So a dist key can have no participant row, and iterating
    // only `report.enemies` would drop its whole skill breakdown. The
    // curated `ei_targets` roster the ei-json adapter walks is a third,
    // differently-filtered list, which makes this the difference between
    // the adapter finding its rows on this block and finding nothing.
    if let Some(dist) = enemy_dist {
        for (&enemy_id, skills) in dist {
            let Some(entity_id) = index.by_enemy_id(enemy_id) else { continue };
            if by_entity.get(entity_id).is_some() {
                continue;
            }
            let by_skill = Some(
                skills
                    .iter()
                    .map(|s| {
                        cats.reference_skill(s.skill_id);
                        (s.skill_id, enemy_skill_row(s))
                    })
                    .collect(),
            );
            by_entity.insert(entity_id, DamageEntity { by_skill, ..DamageEntity::default() });
        }
    }

    DamageBlock { squad: DamageSquad { total: squad_total, dps: squad_dps }, by_entity }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_per_target_by_entity_id_not_by_array_position() {
        // The identity/statistics split's payoff: an enemy player's own row
        // and the damage dealt TO them are the same integer. Impossible in
        // the legacy shape, where enemy statistics were `#[serde(skip)]`.
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats, None, None);

        // agent_addr 4575 / enemy_id 9588 are real ids from the committed
        // `wvw-small.anon.zevtc` fixture (the brief's literal `1`/`9` do not
        // occur in it -- verified by an ad hoc probe over the whole fixture;
        // this player's `damage.per_enemy` genuinely contains this enemy).
        let squad_entity = index.by_agent_addr(4575).expect("squad player resolves");
        let enemy_entity = index.by_enemy_id(9588).expect("enemy resolves");

        let row = block.by_entity.get(squad_entity).expect("squad player has a damage row");
        assert!(
            row.per_target.contains_key(&enemy_entity),
            "per_target is keyed by ENTITY id, so it joins to the enemy's own row"
        );
    }

    #[test]
    fn an_enemy_carries_its_outgoing_damage_on_this_block_not_on_its_identity_row() {
        // The one number `targets[].dpsAll[0]` needs. It lived on
        // `EnemyOut::damage_out` -- `#[serde(skip)]`, so invisible on the
        // native wire and readable only through the side channel; this is
        // the assertion that it is now a first-class native measurement.
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats, None, None);

        let with_damage =
            report.enemies.iter().find(|e| e.damage_out > 0).expect("some enemy dealt damage");
        let entity_id = index.by_enemy_id(with_damage.id).expect("enemy resolves to an entity");
        let row = block.by_entity.get(entity_id).expect("enemy has a damage row");
        assert_eq!(row.total, with_damage.damage_out, "no number may change in this spec");

        // The squad aggregate is a SQUAD aggregate: adding enemy rows to
        // `by_entity` must not fold enemy damage into it.
        let expected: u64 = report.players.iter().map(|p| p.damage.total).sum();
        assert_eq!(block.squad.total, expected, "enemy rows stay out of the squad total");
    }

    #[test]
    fn squad_total_matches_the_legacy_report() {
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats, None, None);
        let expected: u64 = report.players.iter().map(|p| p.damage.total).sum();
        assert_eq!(block.squad.total, expected, "no number may change in this spec");
    }

    #[test]
    fn skill_rows_reference_ids_and_register_them_in_the_catalog() {
        let (report, index) = crate::v1::blocks::tests_support::fixture_report();
        let mut cats = crate::v1::catalogs::CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats, None, None);

        // Same real fixture id as the test above (see its comment).
        let squad_entity = index.by_agent_addr(4575).expect("squad player resolves");
        let row = block.by_entity.get(squad_entity).expect("row");
        let by_skill = row.by_skill.as_ref().expect("the fixture is built with --skill-damage on");
        for (skill_id, skill_row) in by_skill {
            // No name anywhere in the block -- names live in catalogs only.
            let v = serde_json::to_value(skill_row).expect("serializable");
            assert!(v.get("name").is_none(), "a block must never inline a skill name");
            let _ = skill_id;
        }

        let built = cats.finish(&Default::default(), None);
        for skill_id in by_skill.keys() {
            assert!(
                built.skills.contains_key(skill_id),
                "every referenced skill id must resolve in the catalog"
            );
        }
    }

    #[test]
    fn squad_aggregate_excludes_non_squad_friendlies_while_by_entity_keeps_them() {
        // Fix round 1 finding: `squad.total`/`squad.dps` must sum ONLY
        // `Role::Squad` entities, never every friendly player. This is
        // deliberately NOT built on the committed fixture -- every player
        // in it is in-squad, so a fixture-based test cannot distinguish a
        // working filter from a missing one (that's precisely how this
        // defect survived review of the passing suite). Uses the shared
        // two-player helper (one in-squad, one a non-squad friendly) rather
        // than a local hand-built roster, so Tasks 6-8 don't each redefine
        // the same fixture (`tests_support::two_player_report`).
        let (mut report, index) = crate::v1::blocks::tests_support::two_player_report();
        report.players[0].damage.total = 500;
        report.players[0].damage.dps = 50.0;
        report.players[1].damage.total = 300;
        report.players[1].damage.dps = 30.0;
        let mut cats = CatalogBuilder::default();
        let block = build_damage(&report, &index, &mut cats, None, None);

        assert_eq!(block.by_entity.len(), 2, "by_entity is the full roster: squad AND non-squad friendlies");
        assert_eq!(block.squad.total, 500, "squad.total must be the in-squad player's damage only, not the sum of both");
        assert_eq!(block.squad.dps, 50.0, "squad.dps must likewise exclude the non-squad friendly");
    }

    #[test]
    fn an_empty_block_serializes_as_an_empty_map_not_null() {
        let block = DamageBlock::default();
        let v = serde_json::to_value(&block).expect("serializable");
        assert_eq!(v["by_entity"], serde_json::json!({}));
    }
}
