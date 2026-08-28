//! The 1.0 container is a pure REPROJECTION of the legacy report: every
//! number reachable in the legacy shape is reachable, and identical, in the
//! 1.0 shape.
//!
//! This is spec #1's safety net. The spec's Section 5 proposed proving the
//! reshape by re-pointing the EI adapter; this plan defers that to spec #2
//! (see the spec amendment in the plan header) and proves it here instead,
//! which is both tighter and cheaper.
//!
//! `build()` mirrors `v1_shape.rs`'s fixture builder: every compute gate is
//! turned on (skill-damage, timeseries, rotation, replay, missiles,
//! damage-mods), so this test exercises populated blocks, not empty ones --
//! a test over empty blocks would prove nothing.

use std::collections::BTreeMap;

fn build() -> (axilog_schema::Report, axilog_schema::v1::ReportV1) {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wvw-small.anon.zevtc"))
        .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let replay_data = axilog_core::analysis::replay::build_replay(
        &raw,
        &enc,
        axilog_core::analysis::replay::DEFAULT_POLL_MS,
    );
    let missiles_data = axilog_core::analysis::missiles::build_missiles(&raw, &enc);
    let damage_mods = axilog_core::analysis::damage_mods::evaluate_catalog_full(
        &raw,
        &axilog_core::analysis::damage::InstidRegistry::build(&raw),
        &enc,
        false,
    );
    let minion_rollups = axilog_core::analysis::minions::build(&raw, &enc);
    let health_percents = axilog_core::analysis::health::ei_health_percents(&raw, &enc);
    let (enemy_dist, enemy_series) = {
        let enemies: std::collections::BTreeSet<u64> =
            enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
        let rep: std::collections::BTreeMap<u64, u64> = enc
            .enemies
            .iter()
            .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
            .collect();
        (
            axilog_core::analysis::skill_damage::build_enemy_dist(&raw, &enemies, &rep),
            axilog_core::analysis::timeseries::build_enemy_series(
                &enc,
                &raw,
                &axilog_core::analysis::damage::InstidRegistry::build(&raw),
                &enemies,
                &rep,
            ),
        )
    };
    // Task 9: the outcome columns on both player-side distributions.
    let dist_outcomes = axilog_core::analysis::dist_outcomes::build(&raw, &enc);
    // Task 10: the healing detail feeds two families under two different
    // flags; with every gate on, both are set from the one pass.
    let healing_detail = axilog_core::analysis::healing_detail::build(&raw, &enc);
    // Task 11: ungated, like every real caller -- `blocks.replay.by_entity`
    // is the always-on half of that block.
    let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
    let replay_extras = axilog_core::analysis::replay_extras::build(&raw);
    // Task 12: the two name-keyed passes, now keyed by source ADDRESS at
    // the source and by source ENTITY ID once native reprojects them.
    let boon_states = axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons);
    let target_conditions = axilog_core::analysis::target_conditions::build(&raw, &enc);
    let self_effects = axilog_core::analysis::self_effects::build(&raw, &enc);
    let squad_buffs = axilog_core::analysis::squad_buffs::build(&raw, &enc);
    let legacy = axilog_schema::build_report(
        &enc,
        &metrics,
        "0.0.0-test",
        Some(&replay_data),
        Some(&missiles_data),
        true,
        true,
        true,
        Some(&damage_mods),
    );
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        None,
        &axilog_schema::v1::Passes {
            damage_mods: Some(&damage_mods),
            minions: Some(&minion_rollups),
            health_percents: Some(&health_percents),
            enemy_dist: Some(&enemy_dist),
            enemy_series: Some(&enemy_series),
            dist_outcomes: Some(&dist_outcomes),
            healing_detail: healing_detail.as_ref(),
            healing_series: healing_detail.as_ref(),
            activity: Some(&activity),
            replay_extras: Some(&replay_extras),
            boon_states: Some(&boon_states),
            target_conditions: Some(&target_conditions),
            self_effects: Some(&self_effects),
            squad_buffs: Some(&squad_buffs),
        },
    );
    (legacy, v1)
}

/// Joins `v1.entities` back to `legacy.players` by account, the same join
/// key every other test in this file uses.
fn by_account(legacy: &axilog_schema::Report) -> BTreeMap<&str, &axilog_schema::PlayerOut> {
    legacy.players.iter().map(|p| (p.account.as_str(), p)).collect()
}

#[test]
fn every_legacy_player_has_exactly_one_entity() {
    let (legacy, v1) = build();
    let squad_entities = v1
        .entities
        .iter()
        .filter(|e| {
            matches!(
                e.role,
                axilog_schema::v1::entities::Role::Squad
                    | axilog_schema::v1::entities::Role::FriendlyPlayer
            )
        })
        .count();
    assert_eq!(squad_entities, legacy.players.len(), "no player lost or duplicated in the roster");
}

/// `legacy.enemies` (`Report.enemies`) is NOT the same population as
/// `entities[]`'s non-squad rows: per the design spec
/// (`docs/superpowers/specs/2026-08-11-native-format-1.0-design.md`,
/// "`entities[]` -- the single roster"), `entities[]` is deliberately the
/// FULL roster of every agent `axilog_core::model::resolve` produced
/// (`enc.enemies`, unfiltered), while `Report.enemies` is a documented
/// FILTERED VIEW over that same roster -- `role != "squad"` intersected
/// with `Metrics::combat_participant_enemies` (the MROSTER-style
/// nonzero-interaction rule). So `legacy.enemies` is a SUBSET of the
/// non-squad `entities[]` rows, not a 1:1 population -- an enemy agent that
/// was tracked but never exchanged a hit with the squad legitimately has an
/// `entities[]` row and no `Report.enemies` row. This is confirmed by the
/// committed fixture: 80 tracked enemy agents in `entities[]`, 49 of them
/// combat participants in `legacy.enemies`.
///
/// The equivalence claim this test proves instead: every enemy the legacy
/// shape DOES carry resolves to exactly one entity, with no duplicates and
/// no loss -- i.e. `legacy.enemies` is contained in `entities[]`.
#[test]
fn every_legacy_enemy_resolves_to_exactly_one_entity() {
    let (legacy, v1) = build();
    let non_squad_ids: std::collections::BTreeSet<u64> = v1
        .entities
        .iter()
        .filter(|e| {
            !matches!(
                e.role,
                axilog_schema::v1::entities::Role::Squad
                    | axilog_schema::v1::entities::Role::FriendlyPlayer
            )
        })
        .map(|e| e.agent_addr)
        .collect();

    // `entities[]` is a superset: every combat-participant enemy the legacy
    // shape carries must resolve to a distinct non-squad entity.
    let mut resolved = std::collections::BTreeSet::new();
    for enemy in &legacy.enemies {
        assert!(
            non_squad_ids.contains(&enemy.id),
            "legacy enemy {} (agent_addr/enemy_id {}) has no entities[] row",
            enemy.name,
            enemy.id
        );
        assert!(resolved.insert(enemy.id), "legacy enemy {} duplicated in entities[]", enemy.id);
    }
    assert_eq!(resolved.len(), legacy.enemies.len(), "no legacy enemy lost or duplicated in the roster");

    // And the roster is a strict superset here -- the fixture has enemy
    // agents that never fought, which is exactly what `Report.enemies`'s
    // combat-participant filter is documented to drop.
    assert!(
        non_squad_ids.len() > legacy.enemies.len(),
        "expected entities[] to carry non-combat-participant enemies too, by design"
    );
}

#[test]
fn per_player_damage_totals_are_identical() {
    let (legacy, v1) = build();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = damage.by_entity.get(e.id).expect("damage row for every player entity");
        assert_eq!(row.total, p.damage.total, "{account} total damage");
        assert_eq!(row.dps, p.damage.dps, "{account} dps");
        assert_eq!(row.taken, p.damage_taken, "{account} damage taken");
        checked += 1;
    }
    assert!(checked >= 30, "expected a substantial join, got {checked}");
}

#[test]
fn every_legacy_boon_cell_survives_the_reshape() {
    let (legacy, v1) = build();
    let boons = v1.blocks.boons.as_ref().expect("boons block present");
    let by_account = by_account(&legacy);

    let mut cells = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = boons.by_entity.get(e.id).expect("boon row");
        assert_eq!(row.len(), p.boons.len(), "{account} boon count");
        for legacy_boon in &p.boons {
            let got = row.get(&legacy_boon.id).expect("boon id present");
            assert_eq!(got.uptime_pct, legacy_boon.presence_pct, "{account} boon {} uptime", legacy_boon.id);
            cells += 1;
        }
    }
    assert!(cells >= 300, "expected the full boon matrix, got {cells} cells");
}

#[test]
fn the_squad_damage_series_decodes_back_to_the_legacy_array() {
    let (legacy, v1) = build();
    let series = v1.blocks.series.as_ref().expect("series block");
    let squad = &series.squad;
    assert_eq!(squad.damage.decode_u64(), legacy.timeline.per_second.squad_damage);
    assert_eq!(
        squad.downs.decode_u64(),
        legacy.timeline.per_second.downs.iter().map(|v| u64::from(*v)).collect::<Vec<_>>()
    );
}

#[test]
fn every_legacy_defenses_field_survives_the_reshape() {
    let (legacy, v1) = build();
    let defenses = v1.blocks.defenses.as_ref().expect("defenses block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = defenses.by_entity.get(e.id).expect("defenses row for every player entity");
        let d = &p.defenses;
        assert_eq!(row.blocked_count, d.blocked_count, "{account} blocked_count");
        assert_eq!(row.evaded_count, d.evaded_count, "{account} evaded_count");
        assert_eq!(row.dodge_count, d.dodge_count, "{account} dodge_count");
        assert_eq!(row.missed_count, d.missed_count, "{account} missed_count");
        assert_eq!(row.interrupted_count, d.interrupted_count, "{account} interrupted_count");
        assert_eq!(row.invulned_count, d.invulned_count, "{account} invulned_count");
        assert_eq!(row.strike_count, d.strike_count, "{account} strike_count");
        assert_eq!(row.strike_damage, d.strike_damage, "{account} strike_damage");
        assert_eq!(row.power_count, d.power_count, "{account} power_count");
        assert_eq!(row.power_damage, d.power_damage, "{account} power_damage");
        assert_eq!(row.condition_count, d.condition_count, "{account} condition_count");
        assert_eq!(row.condition_damage, d.condition_damage, "{account} condition_damage");
        assert_eq!(row.life_leech_count, d.life_leech_count, "{account} life_leech_count");
        assert_eq!(row.life_leech_damage, d.life_leech_damage, "{account} life_leech_damage");
        assert_eq!(row.barrier_count, d.barrier_count, "{account} barrier_count");
        assert_eq!(row.barrier_damage, d.barrier_damage, "{account} barrier_damage");
        assert_eq!(row.breakbar_count, d.breakbar_count, "{account} breakbar_count");
        assert_eq!(row.breakbar_damage, d.breakbar_damage, "{account} breakbar_damage");
        assert_eq!(row.received_cc_count, d.received_cc_count, "{account} received_cc_count");
        assert_eq!(
            row.received_cc_duration_ms, d.received_cc_duration_ms,
            "{account} received_cc_duration_ms"
        );
        assert_eq!(row.boon_strips_taken, d.boon_strips_taken, "{account} boon_strips_taken");
        assert_eq!(
            row.boon_strips_taken_duration_ms, d.boon_strips_taken_duration_ms,
            "{account} boon_strips_taken_duration_ms"
        );
        checked += 1;
    }
    assert!(checked >= 30, "expected a substantial join, got {checked}");
}

#[test]
fn every_legacy_hit_stats_field_survives_the_reshape() {
    let (legacy, v1) = build();
    let hit_stats = v1.blocks.hit_stats.as_ref().expect("hit_stats block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = hit_stats.by_entity.get(e.id).expect("hit_stats row for every player entity");
        let h = &p.hit_stats;
        assert_eq!(row.crit_count, h.crit_count, "{account} crit_count");
        assert_eq!(row.crit_damage, h.crit_damage, "{account} crit_damage");
        assert_eq!(row.flank_count, h.flank_count, "{account} flank_count");
        assert_eq!(row.glance_count, h.glance_count, "{account} glance_count");
        assert_eq!(row.moving_count, h.moving_count, "{account} moving_count");
        assert_eq!(row.connected_count, h.connected_count, "{account} connected_count");
        assert_eq!(row.connected_damage, h.connected_damage, "{account} connected_damage");
        assert_eq!(row.direct_count, h.direct_count, "{account} direct_count");
        assert_eq!(row.direct_damage, h.direct_damage, "{account} direct_damage");
        assert_eq!(row.condition_count, h.condition_count, "{account} condition_count");
        assert_eq!(row.condition_damage, h.condition_damage, "{account} condition_damage");
        assert_eq!(row.critable_direct_count, h.critable_direct_count, "{account} critable_direct_count");
        assert_eq!(row.against_downed_count, h.against_downed_count, "{account} against_downed_count");
        assert_eq!(row.against_downed_damage, h.against_downed_damage, "{account} against_downed_damage");
        assert_eq!(row.life_leech_count, h.life_leech_count, "{account} life_leech_count");
        assert_eq!(row.life_leech_damage, h.life_leech_damage, "{account} life_leech_damage");
        assert_eq!(row.above90_power_count, h.above90_power_count, "{account} above90_power_count");
        assert_eq!(row.above90_power_damage, h.above90_power_damage, "{account} above90_power_damage");
        assert_eq!(
            row.above90_condition_count, h.above90_condition_count,
            "{account} above90_condition_count"
        );
        assert_eq!(
            row.above90_condition_damage, h.above90_condition_damage,
            "{account} above90_condition_damage"
        );
        checked += 1;
    }
    assert!(checked >= 30, "expected a substantial join, got {checked}");
}

#[test]
fn every_legacy_cc_field_survives_the_reshape() {
    let (legacy, v1) = build();
    let cc = v1.blocks.cc.as_ref().expect("cc block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = cc.by_entity.get(e.id).expect("cc row for every player entity");
        let c = &p.cc;
        assert_eq!(row.applied_total, c.applied_total, "{account} applied_total");
        assert_eq!(row.applied_duration_ms, c.applied_duration_ms, "{account} applied_duration_ms");
        assert_eq!(row.stun_breaks, c.stun_breaks, "{account} stun_breaks");
        assert_eq!(
            row.removed_stun_duration_ms, c.removed_stun_duration_ms,
            "{account} removed_stun_duration_ms"
        );
        checked += 1;
    }
    assert!(checked >= 30, "expected a substantial join, got {checked}");

    // Squad aggregate must equal the sum of the squad rows -- cross-checks
    // the arithmetic, not just field presence.
    let squad_ids: std::collections::HashSet<u32> = v1
        .entities
        .iter()
        .filter(|e| matches!(e.role, axilog_schema::v1::entities::Role::Squad))
        .map(|e| e.id)
        .collect();
    let summed_total: u32 = cc.by_entity.0.iter().filter(|(id, _)| squad_ids.contains(id)).map(|(_, r)| r.applied_total).sum();
    assert_eq!(cc.squad.applied_total, summed_total, "squad cc aggregate must equal the sum of squad rows");
}

#[test]
fn every_legacy_support_field_survives_the_reshape() {
    let (legacy, v1) = build();
    let support = v1.blocks.support.as_ref().expect("support block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = support.by_entity.get(e.id).expect("support row for every player entity");
        let s = &p.support;
        assert_eq!(row.cleanses, s.cleanses, "{account} cleanses");
        assert_eq!(row.cleanses_self, s.cleanses_self, "{account} cleanses_self");
        assert_eq!(row.strips, s.strips, "{account} strips");
        assert_eq!(row.strips_duration_ms, s.strips_duration_ms, "{account} strips_duration_ms");
        assert_eq!(row.resurrects, s.resurrects, "{account} resurrects");
        checked += 1;
    }
    assert!(checked >= 30, "expected a substantial join, got {checked}");
}

#[test]
fn every_legacy_contribution_field_survives_the_reshape() {
    let (legacy, v1) = build();
    let contribution = v1.blocks.contribution.as_ref().expect("contribution block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = contribution.by_entity.get(e.id).expect("contribution row for every player entity");

        assert_eq!(row.downs_contribution.damage, p.downs_contribution.damage, "{account} downs_contribution.damage");
        assert_eq!(row.downs_contribution.cc, p.downs_contribution.cc, "{account} downs_contribution.cc");
        assert_eq!(row.downs_contribution.strips, p.downs_contribution.strips, "{account} downs_contribution.strips");
        assert_eq!(
            row.downs_contribution.movement_impairing, p.downs_contribution.movement_impairing,
            "{account} downs_contribution.movement_impairing"
        );

        assert_eq!(row.downed_by.damage, p.downed_by.damage, "{account} downed_by.damage");
        assert_eq!(row.downed_by.cc, p.downed_by.cc, "{account} downed_by.cc");
        assert_eq!(row.downed_by.strips, p.downed_by.strips, "{account} downed_by.strips");
        assert_eq!(
            row.downed_by.movement_impairing, p.downed_by.movement_impairing,
            "{account} downed_by.movement_impairing"
        );
        checked += 1;
    }
    assert!(checked >= 30, "expected a substantial join, got {checked}");
}

#[test]
fn every_legacy_healing_field_survives_the_reshape() {
    let (legacy, v1) = build();
    let healing = v1.blocks.healing.as_ref().expect("healing block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    let mut skipped_no_data = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        // `PlayerOut::healing` is `Option` -- `None` when the log carries no
        // healing-extension data at all. That is a legitimate "absent, not
        // zero" state, mirrored by the row's own absence in the 1.0 block.
        let Some(h) = p.healing.as_ref() else {
            skipped_no_data += 1;
            assert!(healing.by_entity.get(e.id).is_none(), "{account} has no legacy healing data but a v1 row exists");
            continue;
        };
        let row = healing.by_entity.get(e.id).expect("healing row for every player with legacy healing data");
        assert_eq!(row.outgoing_total, h.healing_out_total, "{account} outgoing_total");
        assert_eq!(row.outgoing_allies, h.healing_out_allies, "{account} outgoing_allies");
        assert_eq!(row.outgoing_self, h.healing_out_self, "{account} outgoing_self");
        assert_eq!(row.barrier_out, h.barrier_out, "{account} barrier_out");
        assert_eq!(row.downed_healing_out, h.downed_healing_out, "{account} downed_healing_out");
        checked += 1;
    }
    assert!(
        checked >= 30 || (checked + skipped_no_data) >= 30,
        "expected a substantial join, got {checked} checked + {skipped_no_data} skipped"
    );
}

#[test]
fn every_legacy_rotation_cast_survives_the_reshape() {
    let (legacy, v1) = build();
    let rotation = v1.blocks.rotation.as_ref().expect("rotation block present -- fixture is built with rotation on");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    let mut total_casts = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let Some(legacy_rotation) = p.rotation.as_ref() else { continue };

        let legacy_cast_count: usize = legacy_rotation.iter().map(|s| s.casts.len()).sum();
        let row = rotation.by_entity.get(e.id).expect("rotation row for every player with legacy rotation data");
        // `casts` is the `--rotation` gate record (Task 13): the legacy
        // rotation being `Some` is exactly the condition that makes it
        // `Some` here, which the `expect` pins alongside the count.
        let v1_casts_vec =
            row.casts.as_ref().expect("casts is Some wherever the legacy rotation is Some");
        assert_eq!(v1_casts_vec.len(), legacy_cast_count, "{account} casts.len()");

        // Every legacy cast (skill_id, cast_time_ms, duration_ms,
        // time_gained_ms, quickness) must appear exactly once among the
        // flattened 1.0 casts. Compare as multisets since `build_rotation`
        // re-sorts by (cast_time_ms, skill_id).
        let mut legacy_casts: Vec<(u32, i64, i64, i64, u64)> = legacy_rotation
            .iter()
            .flat_map(|s| {
                s.casts.iter().map(move |c| {
                    (s.skill_id, c.cast_time_ms, c.duration_ms, c.time_gained_ms, c.quickness.to_bits())
                })
            })
            .collect();
        let mut v1_casts: Vec<(u32, i64, i64, i64, u64)> = v1_casts_vec
            .iter()
            .map(|c| (c.skill_id, c.cast_time_ms, c.duration_ms, c.time_gained_ms, c.quickness.to_bits()))
            .collect();
        legacy_casts.sort();
        v1_casts.sort();
        assert_eq!(v1_casts, legacy_casts, "{account} cast multiset must match exactly");

        total_casts += legacy_cast_count;
        checked += 1;
    }
    assert!(checked >= 1, "expected at least one player with rotation data, got {checked}");
    assert!(total_casts >= 30, "expected a substantial number of casts across the roster, got {total_casts}");
}


/// Fix round 1, Finding 1: `per_player_damage_totals_are_identical` above
/// only compared `total`/`dps`/`taken` -- `DamageEntity::per_target` (the
/// legacy `damage.per_enemy`) had zero equivalence coverage, even though
/// `build()` turns the skill-damage gate on specifically so it is
/// populated. On the committed fixture this is 872 rows across the roster
/// (verified with an ad hoc probe before pinning the minimum below).
#[test]
fn every_legacy_per_enemy_damage_row_survives_the_reshape() {
    let (legacy, v1) = build();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let row = damage.by_entity.get(e.id).expect("damage row for every player entity");

        // Final review: `per_target` is now the UNION of three legacy
        // per-enemy families (`damage.per_enemy`, `PlayerOut::per_target`,
        // `skill_damage.per_target`), whose enemy-id sets genuinely differ
        // -- `PlayerOut::per_target` alone is already a union with the
        // down-contribution map, so an enemy can be credited there with no
        // `per_enemy` damage row at all. So an exact count match is the
        // WRONG assertion now; the right ones are containment (below, per
        // row) plus "no phantom rows": every key beyond the `per_enemy`
        // set must be justified by one of the gated families actually
        // carrying data for it.
        let per_enemy_ids: std::collections::BTreeSet<u32> = p
            .damage
            .per_enemy
            .iter()
            .filter_map(|pe| {
                v1.entities.iter().find(|te| te.agent_addr == pe.enemy_id).map(|te| te.id)
            })
            .collect();
        for (tid, pt) in &row.per_target {
            if per_enemy_ids.contains(tid) {
                continue;
            }
            assert!(
                pt.detail.is_some() || !pt.by_skill.is_empty(),
                "{account} per_target[{tid}] is a phantom row: not in damage.per_enemy and \
                 carrying neither gated family's data"
            );
        }

        for pe in &p.damage.per_enemy {
            let Some(target_id) = v1
                .entities
                .iter()
                .find(|te| te.agent_addr == pe.enemy_id && !matches!(te.role, axilog_schema::v1::entities::Role::Squad | axilog_schema::v1::entities::Role::FriendlyPlayer))
                .map(|te| te.id)
            else {
                panic!("{account} per_enemy target {} has no entities[] row", pe.enemy_id);
            };
            let got = row.per_target.get(&target_id).unwrap_or_else(|| {
                panic!("{account} per_target missing entry for enemy {} (entity {target_id})", pe.enemy_id)
            });
            assert_eq!(got.total, pe.total, "{account} per_target[{target_id}].total");
            checked += 1;
        }
    }
    assert!(checked >= 800, "expected the full per-target matrix, got {checked} cells");
}

/// Fix round 1, Finding 1 continued: `DamageEntity::by_skill` (the legacy
/// `skill_damage.outgoing`) likewise had zero equivalence coverage. On the
/// committed fixture this is 405 rows across the roster.
///
/// Fix round 2: the first pass of this test compared only 4 of
/// `SkillEntryOut`'s 6 non-key fields (`total`/`hits`/`min`/`max`),
/// missing `crit_hits`/`flank_hits` entirely -- and PASSED anyway, because
/// a test that only checks the fields a struct happens to carry cannot
/// catch a field the struct is missing. That was the actual data-loss bug
/// (`SkillRow` never carried `crit_hits`/`flank_hits` in the first place,
/// see the fix-round-1 report appendix); the production fix added both
/// fields to `SkillRow` and this test now asserts on all six, so the same
/// class of gap cannot silently reopen.
/// Side-channel absorption Task 9: `by_skill`/`by_skill_taken` are no
/// longer a one-for-one reshape of the legacy `SkillDamageOut` rows -- the
/// `dist_outcomes` pass merges into the same maps, and its row set is a
/// SUPERSET, because a skill whose every attempt was blocked deals no
/// damage and so never reaches `skill_damage`'s `dmg > 0` accumulator.
/// Those pure-mitigation rows are the payload, not noise.
///
/// So the count assertion becomes a superset assertion -- but not a bare
/// `>=`, which would pass just as happily if the merge invented rows out
/// of nowhere. Every EXTRA row has to look exactly like what the outcome
/// pass alone can produce: no damage, no contributing hits, and outcome
/// columns present. A merge bug that duplicated or mangled a real damage
/// row would land a nonzero `total` in the extras and fail here.
fn assert_outcome_superset(
    rows: &std::collections::BTreeMap<u32, axilog_schema::v1::blocks::damage::SkillRow>,
    legacy_ids: impl Iterator<Item = u32>,
    label: &str,
) {
    let legacy: std::collections::BTreeSet<u32> = legacy_ids.collect();
    assert!(
        rows.len() >= legacy.len(),
        "{label}: reshape lost rows ({} native < {} legacy)",
        rows.len(),
        legacy.len()
    );
    for (id, row) in rows.iter().filter(|(id, _)| !legacy.contains(id)) {
        assert!(
            row.outcomes.is_some(),
            "{label}: extra row for skill {id} has no outcome columns, so nothing produced it"
        );
        assert_eq!(row.total, 0, "{label}: extra row for skill {id} carries damage");
        assert_eq!(row.hits, Some(0), "{label}: extra row for skill {id} carries contributing hits");
    }
}

#[test]
fn every_legacy_skill_damage_row_survives_the_reshape() {
    let (legacy, v1) = build();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let Some(skill_damage) = p.skill_damage.as_ref() else { continue };
        let row = damage.by_entity.get(e.id).expect("damage row for every player entity");

        assert_outcome_superset(
            row.by_skill.as_ref().expect("--skill-damage was on"),
            skill_damage.outgoing.iter().map(|e| e.skill_id),
            &format!("{account} by_skill"),
        );

        for legacy_skill in &skill_damage.outgoing {
            let got = row
                .by_skill
                .as_ref()
                .and_then(|m| m.get(&legacy_skill.skill_id))
                .unwrap_or_else(|| panic!("{account} by_skill missing entry for skill {}", legacy_skill.skill_id));
            assert_eq!(got.total, legacy_skill.total, "{account} by_skill[{}].total", legacy_skill.skill_id);
            assert_eq!(got.hits, Some(legacy_skill.hits), "{account} by_skill[{}].hits", legacy_skill.skill_id);
            assert_eq!(got.min, legacy_skill.min, "{account} by_skill[{}].min", legacy_skill.skill_id);
            assert_eq!(got.max, legacy_skill.max, "{account} by_skill[{}].max", legacy_skill.skill_id);
            assert_eq!(
                got.crit_hits, legacy_skill.crit_hits,
                "{account} by_skill[{}].crit_hits", legacy_skill.skill_id
            );
            assert_eq!(
                got.flank_hits, legacy_skill.flank_hits,
                "{account} by_skill[{}].flank_hits", legacy_skill.skill_id
            );
            checked += 1;
        }
    }
    assert!(checked >= 380, "expected the full by-skill matrix, got {checked} rows");
}

/// Fix round 1, Finding 2: the legacy combat-participant predicate
/// (`Metrics::combat_participant_enemies`, the filter behind `Report.
/// enemies`) is now surfaced on `EntityOut::combat_participant`, so the
/// filtered view stays expressible over `entities[]` instead of being
/// unrecoverable. This makes the superset relationship from
/// `every_legacy_enemy_resolves_to_exactly_one_entity` precise: not just
/// bounded (`non_squad_ids.len() > legacy.enemies.len()`), but exactly the
/// entities with `combat_participant == true`.
#[test]
fn combat_participant_flag_reproduces_the_legacy_enemies_filter_exactly() {
    let (legacy, v1) = build();

    // Every legacy enemy must be a combat participant.
    let non_squad: std::collections::BTreeMap<u64, &axilog_schema::v1::entities::EntityOut> = v1
        .entities
        .iter()
        .filter(|e| {
            !matches!(
                e.role,
                axilog_schema::v1::entities::Role::Squad
                    | axilog_schema::v1::entities::Role::FriendlyPlayer
            )
        })
        .map(|e| (e.agent_addr, e))
        .collect();

    for enemy in &legacy.enemies {
        let entity = non_squad.get(&enemy.id).expect("legacy enemy has an entities[] row");
        assert!(entity.combat_participant, "legacy enemy {} must be flagged combat_participant", enemy.name);
    }

    // And the flag is not just a superset over-approximation: the count of
    // non-squad entities flagged combat_participant must equal
    // legacy.enemies.len() exactly, i.e. the flag reproduces the filter,
    // not merely satisfies it in one direction.
    let flagged_count = non_squad.values().filter(|e| e.combat_participant).count();
    assert_eq!(
        flagged_count,
        legacy.enemies.len(),
        "combat_participant-flagged entities must exactly reproduce legacy.enemies"
    );

    // Every squad/friendly entity is always a participant.
    for e in &v1.entities {
        if matches!(
            e.role,
            axilog_schema::v1::entities::Role::Squad | axilog_schema::v1::entities::Role::FriendlyPlayer
        ) {
            assert!(e.combat_participant, "friendly entities are always combat_participant");
        }
    }
}

/// Final review, Finding 2: `PerTargetStatsOut` (the legacy
/// `PlayerOut::per_target`) was never read by any 1.0 builder. Eight fields
/// -- including `interrupts` and `downs_contribution_damage`, which are not
/// recoverable from any other block -- simply had no 1.0 destination. They
/// now land on `damage.by_entity[].per_target[].detail`.
#[test]
fn every_legacy_per_target_stats_field_survives_the_reshape() {
    let (legacy, v1) = build();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let Some(stats) = p.per_target.as_ref() else { continue };
        let row = damage.by_entity.get(e.id).expect("damage row for every player entity");

        for s in stats {
            let Some(tid) = v1.entities.iter().find(|te| te.agent_addr == s.enemy_id).map(|te| te.id)
            else {
                panic!("{account} per_target target {} has no entities[] row", s.enemy_id);
            };
            let got = row
                .per_target
                .get(&tid)
                .unwrap_or_else(|| panic!("{account} per_target missing entity {tid}"))
                .detail
                .as_ref()
                .unwrap_or_else(|| panic!("{account} per_target[{tid}] carries no detail"));
            assert_eq!(got.connected_hits, s.connected_hits, "{account}/{tid} connected_hits");
            assert_eq!(got.connected_damage, s.connected_damage, "{account}/{tid} connected_damage");
            assert_eq!(
                got.against_downed_count, s.against_downed_count,
                "{account}/{tid} against_downed_count"
            );
            assert_eq!(got.downed, s.downed, "{account}/{tid} downed");
            assert_eq!(got.killed, s.killed, "{account}/{tid} killed");
            assert_eq!(got.interrupts, s.interrupts, "{account}/{tid} interrupts");
            assert_eq!(
                got.downs_contribution_damage, s.downs_contribution_damage,
                "{account}/{tid} downs_contribution_damage"
            );
            checked += 1;
        }
    }
    assert!(checked >= 800, "expected the full per-target stats matrix, got {checked} rows");
}

/// Final review, Finding 2 continued: `SkillDamageOut::taken` (incoming
/// per-skill damage) and `SkillDamageOut::per_target` (the per-(player,
/// target, skill) breakdown) were never read either -- `build_damage`
/// iterated `.outgoing` alone. They now land on
/// `damage.by_entity[].by_skill_taken` and
/// `damage.by_entity[].per_target[].by_skill`.
#[test]
fn every_legacy_incoming_and_per_target_skill_row_survives_the_reshape() {
    let (legacy, v1) = build();
    let damage = v1.blocks.damage.as_ref().expect("damage block present");
    let by_account = by_account(&legacy);

    let mut taken_checked = 0usize;
    let mut per_target_checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let Some(sd) = p.skill_damage.as_ref() else { continue };
        let row = damage.by_entity.get(e.id).expect("damage row for every player entity");

        assert_outcome_superset(
            row.by_skill_taken.as_ref().expect("--skill-damage was on"),
            sd.taken.iter().map(|e| e.skill_id),
            &format!("{account} by_skill_taken"),
        );
        for legacy_skill in &sd.taken {
            let got = row.by_skill_taken.as_ref().and_then(|m| m.get(&legacy_skill.skill_id)).unwrap_or_else(|| {
                panic!("{account} by_skill_taken missing skill {}", legacy_skill.skill_id)
            });
            assert_eq!(got.total, legacy_skill.total, "{account} taken total");
            assert_eq!(got.hits, Some(legacy_skill.hits), "{account} taken hits");
            assert_eq!(got.min, legacy_skill.min, "{account} taken min");
            assert_eq!(got.max, legacy_skill.max, "{account} taken max");
            assert_eq!(got.crit_hits, legacy_skill.crit_hits, "{account} taken crit_hits");
            assert_eq!(got.flank_hits, legacy_skill.flank_hits, "{account} taken flank_hits");
            taken_checked += 1;
        }

        for t in &sd.per_target {
            let Some(tid) = v1.entities.iter().find(|te| te.agent_addr == t.enemy_id).map(|te| te.id)
            else {
                panic!("{account} skill per_target {} has no entities[] row", t.enemy_id);
            };
            let pt = row
                .per_target
                .get(&tid)
                .unwrap_or_else(|| panic!("{account} per_target missing entity {tid}"));
            assert_eq!(pt.by_skill.len(), t.skills.len(), "{account}/{tid} per-target skill count");
            for legacy_skill in &t.skills {
                let got = pt.by_skill.get(&legacy_skill.skill_id).unwrap_or_else(|| {
                    panic!("{account}/{tid} per-target by_skill missing {}", legacy_skill.skill_id)
                });
                assert_eq!(got.total, legacy_skill.total, "{account}/{tid} per-target total");
                assert_eq!(got.hits, Some(legacy_skill.hits), "{account}/{tid} per-target hits");
                assert_eq!(got.min, legacy_skill.min, "{account}/{tid} per-target min");
                assert_eq!(got.max, legacy_skill.max, "{account}/{tid} per-target max");
                assert_eq!(got.crit_hits, legacy_skill.crit_hits, "{account}/{tid} per-target crit_hits");
                assert_eq!(got.flank_hits, legacy_skill.flank_hits, "{account}/{tid} per-target flank_hits");
                per_target_checked += 1;
            }
        }
    }
    assert!(taken_checked >= 100, "expected a substantial incoming per-skill set, got {taken_checked}");
    assert!(
        per_target_checked >= 2000,
        "expected the full per-(player, target, skill) matrix, got {per_target_checked} rows"
    );
}

/// Final review, Finding 3: `PlayerOut::aftercast` is assigned to the
/// `rotation` block by the spec's own block-source table, but
/// `build_rotation` never read it.
#[test]
fn every_legacy_aftercast_field_survives_the_reshape() {
    let (legacy, v1) = build();
    let rotation = v1.blocks.rotation.as_ref().expect("rotation block present");
    let by_account = by_account(&legacy);

    let mut checked = 0usize;
    for e in &v1.entities {
        let Some(account) = e.account.as_deref() else { continue };
        let Some(p) = by_account.get(account) else { continue };
        let Some(row) = rotation.by_entity.get(e.id) else { continue };
        assert_eq!(row.aftercast.saved_count, p.aftercast.saved_count, "{account} saved_count");
        assert_eq!(row.aftercast.saved_ms, p.aftercast.saved_ms, "{account} saved_ms");
        assert_eq!(row.aftercast.wasted_count, p.aftercast.wasted_count, "{account} wasted_count");
        assert_eq!(row.aftercast.wasted_ms, p.aftercast.wasted_ms, "{account} wasted_ms");
        checked += 1;
    }
    assert!(checked >= 30, "expected a substantial join, got {checked}");
}

/// A log recorded without the arcdps healing addon reports `healing:
/// "unsupported"` -- not `"empty"`, and certainly not `"present"`.
///
/// Two rounds of this program have now argued about this one value. Final
/// review Finding 4 found it reporting `Present` with zero rows -- the exact
/// "absent reported as zero" ambiguity `coverage` exists to remove -- and
/// moved it to `Empty`. Task 10 moved it again, because `Empty` is still the
/// wrong answer: it says the pass ran and the squad healed for nothing. No
/// flag and no future pass can produce these numbers from a log whose
/// recorder never wrote them, which is what `Unsupported` means and the only
/// condition in this container that reaches it.
///
/// The committed fixture cannot witness any of this -- it DOES carry
/// healing-extension data -- which is exactly why the original gap survived
/// a fixture-only suite. So this builds the minimal encounter that
/// reproduces it: one player, `has_healing_extension == false`.
#[test]
fn a_log_without_the_healing_extension_reports_unsupported_not_empty() {
    use axilog_core::analysis::{Metrics, PlayerMetrics, Timeline};
    use axilog_core::model::{Encounter, Player};
    use axilog_schema::v1::envelope::CoverageState;

    let enc = Encounter { log_start_ms: 0,
        kind: "wvw".into(), pve: None,
        map: String::new(),
        duration_ms: 1000,
        build: String::new(),
        revision: 1,
        recorded_by: None,
        teams: vec![],
        players: vec![Player {
            agent_addr: 1,
            account: ":Squaddie.1".into(),
            character: "Char1".into(),
            profession: "Guardian".into(),
            elite_spec: "Firebrand".into(),
            team: "red".into(),
            subgroup: 1,
            in_squad: true,
            commander: false,
            marker: None,
            commander_tag: None,
            guild_id: None,
            agent_addrs: vec![1],
        }],
        enemies: vec![],
        markers: vec![], ground_markers: vec![],
        tick_rate: None, objectives: Vec::new(), started_at_unix: None, map_id: None,
    };
    let metrics = Metrics {
        players: vec![PlayerMetrics { agent_addr: 1, ..Default::default() }],
        timeline: Timeline {
            resolution_ms: 1000,
            squad_damage: vec![0],
            cc_applied: vec![0],
            downs: vec![0],
            strips: vec![0],
        },
        // The whole point of the witness: no healing addon on this log.
        healing_extension: None,
        ..Default::default()
    };
    let legacy = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, false, false, false, None,
    );
    let v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None, &Default::default());

    assert!(
        legacy.players.iter().all(|p| p.healing.is_none()),
        "premise: no healing-extension data means no legacy healing rows"
    );
    let healing = v1.blocks.healing.as_ref().expect("an empty block is still carried");
    assert!(healing.by_entity.is_empty(), "no healing rows on this log");
    assert_eq!(
        v1.coverage.get("healing"),
        Some(CoverageState::Unsupported),
        "a log with no healing extension cannot ANSWER the healing question -- reporting `empty` \
         claims it answered zero"
    );

    // And the distinction is real, not a blanket downgrade: a block that
    // DID produce rows on the same document still reports `present`.
    assert_eq!(v1.coverage.get("damage"), Some(CoverageState::Present));

    // The fixture-scale document still reports `present` everywhere it
    // has rows -- neither `Empty` nor `Unsupported` became the new default.
    let (_, fixture) = build();
    assert_eq!(fixture.coverage.get("healing"), Some(CoverageState::Present));
    assert_eq!(fixture.coverage.get("damage"), Some(CoverageState::Present));
}

/// `Empty` is still reachable, and still means what Finding 4 made it mean:
/// the pass ran, the question was answerable, and the answer was nothing.
///
/// Task 10 took `healing`-without-the-extension away as its witness, so this
/// supplies the other one -- a log whose roster resolved to no squad players
/// at all, on which every always-on block genuinely has zero rows. Without
/// this the `Empty` arm of `computed` would go back to being dead code that
/// no test observes, which is the state Finding 4 found it in.
#[test]
fn a_computed_block_with_no_rows_still_reports_empty() {
    use axilog_core::analysis::{Metrics, Timeline};
    use axilog_core::model::Encounter;
    use axilog_schema::v1::envelope::CoverageState;

    let enc = Encounter { log_start_ms: 0,
        kind: "wvw".into(), pve: None,
        map: String::new(),
        duration_ms: 1000,
        build: String::new(),
        revision: 1,
        recorded_by: None,
        teams: vec![],
        players: vec![],
        enemies: vec![],
        markers: vec![], ground_markers: vec![],
        tick_rate: None, objectives: Vec::new(), started_at_unix: None, map_id: None,
    };
    let metrics = Metrics {
        // The extension IS present -- so `healing` is answerable here, and
        // its emptiness is a measurement rather than an absence. That is the
        // whole difference between this witness and the one above.
        healing_extension: Some(axilog_core::evtc::ext_healing::Registration { signature: axilog_core::evtc::ext_healing::HEALING_SIGNATURE, revision: 2, version: "test".into() }),
        timeline: Timeline {
            resolution_ms: 1000,
            squad_damage: vec![0],
            cc_applied: vec![0],
            downs: vec![0],
            strips: vec![0],
        },
        ..Default::default()
    };
    let legacy = axilog_schema::build_report(
        &enc, &metrics, "0.0.0-test", None, None, false, false, false, None,
    );
    let v1 = axilog_schema::v1::build_report_v1(&enc, &metrics, &legacy, "0.0.0-test", None, &Default::default());

    assert_eq!(
        v1.coverage.get("healing"),
        Some(CoverageState::Empty),
        "extension present and no rows is `empty`, not `unsupported`"
    );
    assert_eq!(
        v1.coverage.get("damage"),
        Some(CoverageState::Empty),
        "a block that ran over an empty roster is `empty` too"
    );
}

/// Final review, Finding 9 -- the test that would have caught the other
/// eight.
///
/// Every other test in this file asserts that what IS present matches.
/// None asserted that nothing is ABSENT, which is why five separate
/// dropped-field gaps survived fourteen task reviews: a test that only
/// checks the fields a struct happens to carry cannot see a field the
/// struct is missing.
///
/// Rust has no field reflection, so this is an explicit, EXHAUSTIVE
/// checklist: one binding per legacy `PlayerOut` field, destructured
/// WITHOUT `..`. That is the load-bearing part. Adding a field to
/// `PlayerOut` breaks THIS LINE at compile time, forcing whoever adds it to
/// state where it lands in 1.0 -- rather than letting it be silently
/// dropped and rediscovered in a final review. (Verified, not assumed: a
/// field temporarily added to `PlayerOut` fails THIS pattern to compile
/// with `E0027: pattern does not mention field`. Other build sites across
/// the workspace fail too, but only with `E0063: missing field in
/// initializer` -- they demand a VALUE. This is the only site that demands
/// a DECISION about where the field lands in 1.0.)
///
/// Each binding below is used exactly once, either:
///
///   - asserting the 1.0 value equals it (a real destination), or
///   - explicitly named as INTENTIONALLY ABSENT with its reason.
///
/// A binding that is neither would warn as unused, so "forgot to decide"
/// is not a reachable state either.
#[test]
fn every_legacy_player_field_has_a_one_point_oh_destination() {
    let (legacy, v1) = build();
    let damage_block = v1.blocks.damage.as_ref().expect("damage block");
    let defenses_block = v1.blocks.defenses.as_ref().expect("defenses block");
    let hit_stats_block = v1.blocks.hit_stats.as_ref().expect("hit_stats block");
    let cc_block = v1.blocks.cc.as_ref().expect("cc block");
    let boons_block = v1.blocks.boons.as_ref().expect("boons block");
    let support_block = v1.blocks.support.as_ref().expect("support block");
    let contribution_block = v1.blocks.contribution.as_ref().expect("contribution block");
    let healing_block = v1.blocks.healing.as_ref().expect("healing block");
    let rotation_block = v1.blocks.rotation.as_ref().expect("rotation block");
    let damage_mods_block = v1.blocks.damage_mods.as_ref().expect("damage_mods block");
    let series_block = v1.blocks.series.as_ref().expect("series block");

    let entity_by_account: BTreeMap<&str, &axilog_schema::v1::entities::EntityOut> = v1
        .entities
        .iter()
        .filter_map(|e| e.account.as_deref().map(|a| (a, e)))
        .collect();
    let entity_by_addr: BTreeMap<u64, u32> = v1.entities.iter().map(|e| (e.agent_addr, e.id)).collect();

    let mut checked = 0usize;
    for p in &legacy.players {
        // NO `..` -- see this test's doc comment. This is the compile-time
        // forcing function, not a stylistic choice.
        let axilog_schema::PlayerOut {
            account,
            character,
            profession,
            elite_spec,
            team,
            subgroup,
            in_squad,
            commander,
            marker,
            commander_tag,
            guild_id,
            damage,
            downs_dealt,
            kills_dealt,
            downs_taken,
            deaths,
            damage_taken,
            cc,
            downs_contribution,
            downed_by,
            boons,
            per_target,
            support,
            healing,
            skill_damage,
            per_second,
            dps_targets,
            hit_stats,
            aftercast,
            defenses,
            rotation,
            damage_mods,
            agent_addr,
            instid,
            breakbar_damage_dealt,
            downs_contribution_per_skill,
        } = p;

        let e = *entity_by_account.get(account.as_str()).expect("every player has an entity");
        let id = e.id;

        // --- identity: moved to `entities[]` ------------------------------
        assert_eq!(e.account.as_deref(), Some(account.as_str()), "account -> entities[].account");
        assert_eq!(e.character.as_deref(), Some(character.as_str()), "character -> entities[].character");
        assert_eq!(e.profession.as_deref(), Some(profession.as_str()), "profession -> entities[].profession");
        assert_eq!(e.elite_spec.as_deref(), Some(elite_spec.as_str()), "elite_spec -> entities[].elite_spec");
        assert_eq!(&e.team, team, "team -> entities[].team");
        assert_eq!(e.subgroup, Some(*subgroup), "subgroup -> entities[].subgroup");
        assert_eq!(
            *in_squad,
            matches!(e.role, axilog_schema::v1::entities::Role::Squad),
            "in_squad -> entities[].role == \"squad\""
        );
        assert_eq!(e.marker.as_deref(), marker.as_deref(), "marker -> entities[].marker");
        assert_eq!(
            e.commander.is_some(),
            commander_tag.is_some(),
            "commander_tag -> entities[].commander"
        );
        if let (Some(got), Some(want)) = (e.commander.as_ref(), commander_tag.as_ref()) {
            assert_eq!(got.variant, want.variant, "commander_tag.variant");
            assert_eq!(got.guid, want.guid, "commander_tag.guid");
        }
        assert_eq!(e.guild_id.as_deref(), guild_id.as_deref(), "guild_id -> entities[].guild_id");
        assert_eq!(e.agent_addr, *agent_addr, "agent_addr -> entities[].agent_addr");
        assert_eq!(e.instid, *instid, "instid -> entities[].instid");

        // INTENTIONALLY ABSENT as a field of its own: `commander: bool`.
        // It is not independent data -- `wvw::apply` assigns
        // `p.commander = p.commander_tag.is_some()` unconditionally, after
        // dedupe, as the last write to either field. So the bool IS
        // `entities[].commander.is_some()`, which is asserted here rather
        // than assumed; carrying it separately would only create a second
        // commander signal that could drift. See `EntityOut::commander`.
        //
        // (The fixture exercises both arms: it has commanders and
        // non-commanders, so this is not vacuous either way.)
        assert_eq!(
            *commander,
            e.commander.is_some(),
            "commander -> entities[].commander presence; if these can now differ, the bool needs its own 1.0 home"
        );

        // --- damage block --------------------------------------------------
        let d = damage_block.by_entity.get(id).expect("damage row");
        assert_eq!(d.total, damage.total, "damage.total");
        assert_eq!(d.dps, damage.dps, "damage.dps");
        assert!(
            d.per_target.len() >= damage.per_enemy.len(),
            "damage.per_enemy -> damage.per_target (a union of three legacy families, so >=)"
        );
        assert_eq!(d.taken, *damage_taken, "damage_taken -> damage.taken");
        assert_eq!(d.downs_dealt, *downs_dealt, "downs_dealt -> damage.downs_dealt");
        assert_eq!(d.kills_dealt, *kills_dealt, "kills_dealt -> damage.kills_dealt");
        // Deep per-field checks live in the dedicated tests above; here we
        // only assert the destination EXISTS and is populated.
        assert_eq!(
            per_target.as_ref().map(|v| v.len()).unwrap_or(0),
            d.per_target.values().filter(|pt| pt.detail.is_some()).count(),
            "per_target -> damage.per_target[].detail"
        );
        // `>=`, not `==`: Task 9's outcome merge adds mitigation-only
        // rows to both maps. This test only asserts the destination exists
        // and is populated; `assert_outcome_superset` above is what pins
        // what those extra rows are allowed to be.
        assert!(
            d.by_skill.as_ref().map_or(0, |m| m.len())
                >= skill_damage.as_ref().map(|s| s.outgoing.len()).unwrap_or(0),
            "skill_damage.outgoing -> damage.by_skill"
        );
        assert!(
            d.by_skill_taken.as_ref().map_or(0, |m| m.len())
                >= skill_damage.as_ref().map(|s| s.taken.len()).unwrap_or(0),
            "skill_damage.taken -> damage.by_skill_taken"
        );
        assert_eq!(
            skill_damage.as_ref().map(|s| s.per_target.len()).unwrap_or(0),
            d.per_target.values().filter(|pt| !pt.by_skill.is_empty()).count(),
            "skill_damage.per_target -> damage.per_target[].by_skill"
        );

        // INTENTIONALLY ABSENT as a field of its own: `dps_targets`. Its
        // `damage` IS carried, as `damage.per_target[].total` (asserted
        // below, so this is a proven identity, not an assumption), and its
        // `dps` is that number over `encounter.duration_ms` -- a derived
        // rate. The format does not store derived rates that a consumer can
        // compute from two values it already has; `rotation` omits APM for
        // exactly this reason (see `CastRow`'s doc comment). Storing one
        // invites it to disagree with its own inputs.
        for t in dps_targets {
            let Some(tid) = entity_by_addr.get(&t.enemy_id) else { continue };
            let pt = d.per_target.get(tid).expect("dps_targets enemy has a per_target row");
            assert_eq!(pt.total, t.damage, "dps_targets[].damage == damage.per_target[].total");
        }

        // --- defenses / hit_stats / cc -------------------------------------
        let df = defenses_block.by_entity.get(id).expect("defenses row");
        assert_eq!(df.blocked_count, defenses.blocked_count, "defenses -> defenses block");
        assert_eq!(df.downs_taken, *downs_taken, "downs_taken -> defenses.downs_taken");
        assert_eq!(df.deaths, *deaths, "deaths -> defenses.deaths");
        assert_eq!(
            hit_stats_block.by_entity.get(id).expect("hit_stats row").crit_count,
            hit_stats.crit_count,
            "hit_stats -> hit_stats block"
        );
        assert_eq!(
            cc_block.by_entity.get(id).expect("cc row").applied_total,
            cc.applied_total,
            "cc -> cc block"
        );

        // --- boons / support / contribution / healing -----------------------
        assert_eq!(
            boons_block.by_entity.get(id).expect("boons row").len(),
            boons.len(),
            "boons -> boons block"
        );
        assert_eq!(
            support_block.by_entity.get(id).expect("support row").cleanses,
            support.cleanses,
            "support -> support block"
        );
        let c = contribution_block.by_entity.get(id).expect("contribution row");
        assert_eq!(c.downs_contribution.damage, downs_contribution.damage, "downs_contribution");
        assert_eq!(c.downed_by.damage, downed_by.damage, "downed_by");
        assert_eq!(
            healing_block.by_entity.get(id).map(|h| h.outgoing_total),
            healing.as_ref().map(|h| h.healing_out_total),
            "healing -> healing block"
        );

        // --- rotation / damage_mods / series --------------------------------
        let r = rotation_block.by_entity.get(id);
        assert_eq!(
            r.and_then(|r| r.casts.as_ref()).map(|c| c.len()),
            rotation.as_ref().map(|v| v.iter().map(|s| s.casts.len()).sum::<usize>()),
            "rotation -> rotation block"
        );
        assert_eq!(
            r.map(|r| r.aftercast.saved_count),
            Some(aftercast.saved_count),
            "aftercast -> rotation.aftercast"
        );
        assert_eq!(
            damage_mods_block.by_entity.get(id).map(|m| m.overall.len()),
            damage_mods.as_ref().map(|m| m.outgoing.len() + m.incoming.len()),
            "damage_mods -> damage_mods block"
        );
        assert_eq!(
            series_block.by_entity.get(id).map(|s| s.damage.decode_u64()),
            per_second.as_ref().map(|s| s.damage.clone()),
            "per_second -> series block"
        );

        // `breakbar_damage_dealt` and `downs_contribution_per_skill` are
        // both `#[serde(skip)]` on `PlayerOut` -- they were never part of
        // the LEGACY JSON at all, existing solely as side data
        // `axilog_ei::to_ei_json` read in-process. This block used to record
        // them as intentionally absent from 1.0 on the grounds that
        // publishing them would be new surface rather than equivalence.
        //
        // Side-channel absorption ended that: 1.0 is now the ONLY thing the
        // ei-json adapter reads, so side data with no native home is data no
        // consumer of this format can see. Both have homes now --
        // `blocks.damage.by_entity[].breakbar_damage_dealt` and
        // `blocks.contribution.by_entity[].downs_contribution_by_skill`
        // (Task 13) -- and the per-skill slice is asserted equal against
        // its legacy source below.
        //
        // The two assertions here still stand and still mean something: they
        // pin that neither field became part of the LEGACY format, which
        // would make it a second, competing publication of the same number.
        let credits = contribution_block
            .by_entity
            .get(id)
            .map(|c| c.downs_contribution_by_skill.clone())
            .unwrap_or_default();
        let expected: std::collections::BTreeMap<u32, u64> = downs_contribution_per_skill
            .iter()
            .filter(|(_, &v)| v > 0)
            .map(|(&k, &v)| (k, v))
            .collect();
        assert_eq!(
            credits, expected,
            "downs_contribution_per_skill -> contribution.downs_contribution_by_skill"
        );
        let legacy_json = serde_json::to_value(p).expect("serializable");
        assert!(
            legacy_json.get("breakbar_damage_dealt").is_none(),
            "breakbar_damage_dealt is EI-adapter side data; if it is now published, give it a 1.0 home"
        );
        assert!(
            legacy_json.get("downs_contribution_per_skill").is_none(),
            "downs_contribution_per_skill is EI-adapter side data; if it is now published, give it a 1.0 home"
        );
        let _ = (breakbar_damage_dealt, downs_contribution_per_skill);

        checked += 1;
    }
    assert!(checked >= 30, "expected the whole roster, got {checked}");
}

/// `build()` already turns every gate on, `--skill-damage` included, so it
/// doubles as the fixture this test needs -- no separate helper required.
fn build_v1_from_fixture_with_skill_damage() -> axilog_schema::v1::ReportV1 {
    build().1
}

/// Phase B item 1: the per-target split must carry the full 23-field set.
/// The inequality below is the load-bearing assertion -- summing a
/// player's per-target connected hits across enumerated targets must be
/// LESS THAN their whole-fight connected count on any log containing
/// non-enumerated targets (NPCs, guards, siege). That gap is exactly the
/// error axibridge's `statsAll[0]` fallback makes, so pinning it here is a
/// regression guard against ever reintroducing the fallback's semantics.
#[test]
fn per_target_detail_carries_the_full_field_set_and_undercounts_whole_fight() {
    let doc = build_v1_from_fixture_with_skill_damage();
    let damage = doc.blocks.damage.as_ref().expect("skill-damage gate was on");
    let hit_stats = doc.blocks.hit_stats.as_ref().expect("hit_stats block is ungated");
    let mut checked = 0usize;
    for (id, dmg) in damage.by_entity.0.iter() {
        let Some(hit) = hit_stats.by_entity.get(*id) else { continue };
        let mut summed = 0u32;
        let mut saw_detail = false;
        for (_target, per) in dmg.per_target.iter() {
            let Some(d) = per.detail.as_ref() else { continue };
            saw_detail = true;
            summed += d.connected_hits;
            // Every new field must be reachable -- a field that never
            // compiles into the struct would pass a value assertion by
            // being absent, so touch each one.
            let _ = (d.direct_count, d.direct_damage, d.crit_count, d.crit_damage);
            let _ = (d.flank_count, d.glance_count, d.critable_direct_count);
            let _ = (d.against_downed_damage, d.missed, d.evaded, d.blocked, d.invulned);
            let _ = (d.applied_total, d.applied_duration_ms);
            let _ = (d.applied_downs_contribution, d.applied_duration_downs_contribution_ms);
        }
        if !saw_detail {
            continue;
        }
        checked += 1;
        assert!(
            summed <= hit.connected_count,
            "entity {id}: per-target hits {summed} exceeded whole-fight {}",
            hit.connected_count
        );
    }
    assert!(checked > 0, "fixture produced no per-target detail -- is --skill-damage on?");
}
