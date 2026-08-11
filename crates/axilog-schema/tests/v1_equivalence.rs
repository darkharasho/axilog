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
        Some(&damage_mods),
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
    let squad = series.squad.as_ref().expect("squad series");
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
        assert_eq!(row.cast_count as usize, legacy_cast_count, "{account} cast_count");
        assert_eq!(row.casts.len(), legacy_cast_count, "{account} casts.len()");

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
        let mut v1_casts: Vec<(u32, i64, i64, i64, u64)> = row
            .casts
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
