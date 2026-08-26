//! Side-channel absorption Task 10: `healing_detail` -> the per-ally and
//! per-skill breakdowns on `blocks.healing`, and the 1S graph on
//! `blocks.series`.
//!
//! Task 10 is the first absorption whose ONE pass lands in TWO blocks under
//! TWO different flags. That split is the whole risk surface, and most of
//! what these tests pin: a family emitted under a flag that did not ask for
//! it, a family silently dropped because it rode the other one, a graph
//! landing on the wrong grid, an ally matrix re-densified against the wrong
//! index.
//!
//! The row-set question Task 9 got wrong does NOT recur here, and
//! `the_detail_enriches_rows_that_already_exist` says why in the one place
//! it could have.

mod common;

use axilog_schema::v1::entities::Role;

fn is_squad(role: Role) -> bool {
    matches!(role, Role::Squad | Role::FriendlyPlayer)
}

/// Unlike Task 9's `dist_outcomes`, this pass adds NO rows: it enriches the
/// ones `blocks.healing` already has.
///
/// That is a claim about the source data, not a hope. `PlayerOut::healing`
/// is `Some` for every player exactly when `Metrics::has_healing_extension`
/// is true (`schema/src/lib.rs`'s `build_report`), and `healing_detail`
/// returns `Some` under that same condition -- so on any log where either
/// exists, both do, for every player. This test is what would fail if that
/// ever stopped being true, which is when a union like `merge_outcomes`
/// would become necessary here too.
#[test]
fn the_detail_enriches_rows_that_already_exist() {
    let (_enc, _metrics, legacy, v1) = common::fixture_report();
    let healing = v1.blocks.healing.as_ref().expect("healing block present");

    let mut annotated = 0usize;
    for e in v1.entities.iter().filter(|e| is_squad(e.role)) {
        let Some(row) = healing.by_entity.get(e.id) else {
            continue;
        };
        assert!(
            row.detail.is_some(),
            "entity {}: a healing row in a gated-on document has no detail, so the merge missed it",
            e.id
        );
        annotated += 1;
    }
    assert_eq!(
        annotated,
        legacy.players.len(),
        "every player has a healing row, and every healing row is annotated -- no extra rows and \
         no missed ones"
    );
}

/// The ally matrix is SPARSE, and its absent cells are measured zeros.
///
/// GW2EI's `outgoingHealingAllies` is a dense N*N array of objects. Keying
/// it by the ally's entity id and dropping all-zero cells is what keeps it
/// from growing quadratically on the native wire -- a real squad heals a
/// small fraction of the roster. If this ever stops being sparse the
/// payload argument behind the `--skill-damage` gate is gone, so the test
/// asserts the sparsity rather than assuming it.
#[test]
fn the_ally_matrix_is_sparse_and_never_exceeds_the_scalar() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let healing = v1.blocks.healing.as_ref().expect("healing block present");
    let squad = v1.entities.iter().filter(|e| is_squad(e.role)).count();

    let mut cells = 0usize;
    let mut rows = 0usize;
    for e in v1.entities.iter().filter(|e| is_squad(e.role)) {
        let Some(d) = healing.by_entity.get(e.id).and_then(|r| r.detail.as_ref()) else {
            continue;
        };
        let row = healing.by_entity.get(e.id).expect("row");
        cells += d.by_ally.len();
        rows += 1;
        for (ally, cell) in &d.by_ally {
            assert!(
                !(cell.healing == 0 && cell.downed_healing == 0 && cell.barrier == 0),
                "entity {} ally {ally}: an all-zero cell was stored instead of omitted",
                e.id
            );
            assert!(
                cell.downed_healing <= cell.healing,
                "entity {} ally {ally}: downed healing is a SUBSET of healing",
                e.id
            );
        }
        // The matrix can only ever be a subset of the scalar: heals landing
        // on friendlies who are not enumerated squad players are in the
        // scalar and in no ally row.
        let ally_total: u64 = d.by_ally.values().map(|c| c.healing).sum();
        assert!(
            ally_total <= row.outgoing_total,
            "entity {}: ally matrix ({ally_total}) exceeds the scalar total ({})",
            e.id,
            row.outgoing_total
        );
    }
    assert!(rows > 0, "no annotated rows to check");
    let dense = rows * squad;
    assert!(
        cells * 2 < dense,
        "the ally matrix stored {cells} of {dense} possible cells -- it is no longer sparse, so \
         the payload argument for gating it on --skill-damage no longer holds"
    );
}

/// `sum(by_skill[*].total) == outgoing_total`, EXACTLY.
///
/// `healing_detail` and the `HealingMetrics` scalars reduce the same
/// `healing::attributed_events` list, so this holds by construction rather
/// than by coincidence -- which is precisely why it is worth asserting: it
/// is the cheapest statement that the two halves did not drift apart in
/// the reshape.
#[test]
fn the_per_skill_dists_sum_to_their_scalars() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let healing = v1.blocks.healing.as_ref().expect("healing block present");

    let mut checked = 0usize;
    for e in v1.entities.iter().filter(|e| is_squad(e.role)) {
        let Some(row) = healing.by_entity.get(e.id) else {
            continue;
        };
        let Some(d) = row.detail.as_ref() else {
            continue;
        };
        assert_eq!(
            d.by_skill.values().map(|r| r.total).sum::<u64>(),
            row.outgoing_total,
            "entity {}: totalHealingDist must sum to the outgoing scalar",
            e.id
        );
        assert_eq!(
            d.barrier_by_skill.values().map(|r| r.total).sum::<u64>(),
            row.barrier_out,
            "entity {}: totalBarrierDist must sum to the barrier scalar",
            e.id
        );
        for (skill, r) in d.by_skill.iter().chain(d.barrier_by_skill.iter()) {
            assert!(
                r.hits > 0,
                "entity {} skill {skill}: a dist row exists only for real events",
                e.id
            );
            assert!(
                r.min <= r.max,
                "entity {} skill {skill}: min exceeds max",
                e.id
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no annotated rows to check");
}

/// A barrier row never carries a downed column.
///
/// GW2EI's `EXTJsonBarrierDist` has no downed field at all, so a zero one
/// here would invent a measurement it does not make. `total_downed` is
/// `skip_serializing_if` zero for exactly that reason, and this pins both
/// halves: the value is zero, and the key is gone from the wire.
#[test]
fn a_barrier_row_carries_no_downed_column() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let healing = v1.blocks.healing.as_ref().expect("healing block present");

    let mut barrier_rows = 0usize;
    for e in &v1.entities {
        let Some(d) = healing.by_entity.get(e.id).and_then(|r| r.detail.as_ref()) else {
            continue;
        };
        for (skill, r) in &d.barrier_by_skill {
            assert_eq!(r.total_downed, 0, "entity {} barrier skill {skill}", e.id);
            let wire = serde_json::to_value(r).expect("serializes");
            assert!(
                wire.get("total_downed").is_none(),
                "entity {} barrier skill {skill}: the zero downed column reached the wire",
                e.id
            );
            barrier_rows += 1;
        }
    }
    assert!(
        barrier_rows > 0,
        "no barrier rows on the fixture -- the omission is untested"
    );
}

/// Every skill id either dist joins on resolves in the catalog.
///
/// The same dangling-reference hole Task 9 found on the damage side: the
/// ei-json adapter used to build these rows from a private side channel
/// that could not reach the catalog, so their ids named nothing. Measured
/// on the committed fixture, absorbing this pass adds 43 entries to
/// `skillMap`, every one of which the OLD document's own healing dists
/// already referenced.
#[test]
fn every_dist_skill_id_is_catalogued() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let healing = v1.blocks.healing.as_ref().expect("healing block present");

    let mut ids = 0usize;
    for e in &v1.entities {
        let Some(d) = healing.by_entity.get(e.id).and_then(|r| r.detail.as_ref()) else {
            continue;
        };
        for skill in d.by_skill.keys().chain(d.barrier_by_skill.keys()) {
            assert!(
                v1.catalogs.skills.contains_key(skill),
                "entity {} references skill {skill}, which has no catalog entry",
                e.id
            );
            ids += 1;
        }
    }
    assert!(ids > 0, "no dist ids on the fixture");
}

/// The 1S graph lives on `blocks.series`, on the SAME grid as its
/// neighbours there -- which is the reason it lives there.
///
/// `healing_detail` buckets on `timeseries::ei_grid` (ceiling), and so do
/// the per-entity damage series; the SQUAD timeline uses a floor grid one
/// bucket shorter. Had this array gone on `blocks.healing` beside the rest
/// of the detail, nothing would have forced it onto either grid, and a
/// consumer zipping it against `damage` would have been off by one bucket
/// on every partial-second log. The length equality below is what forces
/// it.
#[test]
fn the_1s_graph_shares_the_grid_of_the_series_it_sits_beside() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let series = v1.blocks.series.as_ref().expect("series block present");
    let healing = v1.blocks.healing.as_ref().expect("healing block present");

    let mut checked = 0usize;
    for e in v1.entities.iter().filter(|e| is_squad(e.role)) {
        let Some(row) = series.by_entity.get(e.id) else {
            continue;
        };
        let Some(graph) = row.healing_1s.as_ref() else {
            continue;
        };
        assert_eq!(
            graph.len, row.damage.len,
            "entity {}: the healing graph is on a different grid than the damage series beside it",
            e.id
        );
        let decoded = graph.decode_u64();
        assert!(
            decoded.windows(2).all(|w| w[0] <= w[1]),
            "entity {}: the graph is CUMULATIVE and must never decrease",
            e.id
        );
        // The shared-producer invariant again, from the other direction:
        // the graph's last bucket is the same number the scalar carries.
        let scalar = healing
            .by_entity
            .get(e.id)
            .map(|h| h.outgoing_total)
            .unwrap_or(0);
        assert_eq!(
            decoded.last().copied().unwrap_or(0),
            scalar,
            "entity {}: the graph's final bucket must equal the outgoing scalar",
            e.id
        );
        checked += 1;
    }
    assert!(checked > 0, "no healing graphs on the fixture");
}

/// An ENEMY series row carries no healing graph.
///
/// The pass runs over squad players only, so a zero-filled array here would
/// claim an enemy was measured and healed for nothing. Same rule, same
/// reason, as `power_damage` being `Some` only on enemy rows -- the two are
/// mirror images, and between them every `EntitySeries` row has at least
/// one field whose absence is a real statement.
#[test]
fn enemy_series_rows_carry_no_healing_graph() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report();
    let series = v1.blocks.series.as_ref().expect("series block present");

    let mut enemy_rows = 0usize;
    for e in v1.entities.iter().filter(|e| !is_squad(e.role)) {
        let Some(row) = series.by_entity.get(e.id) else {
            continue;
        };
        assert!(
            row.healing_1s.is_none(),
            "entity {}: an enemy row picked up a healing graph from a squad-only pass",
            e.id
        );
        enemy_rows += 1;
    }
    assert!(enemy_rows > 0, "no enemy series rows on the fixture");
}

/// **The split gate, which is what makes Task 10 different from every
/// absorption before it.**
///
/// One pass, two families, two flags: the ally matrix and the two dists
/// ride `--skill-damage` (they grow quadratically in squad size), the 1S
/// graph rides `--timeseries` with every other per-second array. Every
/// task before this one could answer "did the gate run?" from a single
/// presence check, because a pass fed exactly one family.
///
/// The committed fixture builders only offer all-gates-on and
/// all-gates-off, so this constructs the two partial states directly --
/// which is the only way to observe a family being emitted under a flag
/// that did not ask for it.
#[test]
fn each_family_rides_only_its_own_flag() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/wvw-small.anon.zevtc"
    ))
    .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let detail = axilog_core::analysis::healing_detail::build(&raw, &enc)
        .expect("the committed fixture carries the healing extension");

    // Both legacy flags stay ON so that `per_second` exists in both arms:
    // this test is about the two NATIVE fields, and a missing series row
    // would make the graph absent for an unrelated reason.
    let legacy = axilog_schema::build_report(
        &enc,
        &metrics,
        "0.0.0-test",
        None,
        None,
        true,
        true,
        false,
        None,
    );
    let build = |dist: bool, graph: bool| {
        axilog_schema::v1::build_report_v1(
            &enc,
            &metrics,
            &legacy,
            "0.0.0-test",
            None,
            &axilog_schema::v1::Passes {
                healing_detail: dist.then_some(&detail),
                healing_series: graph.then_some(&detail),
                ..Default::default()
            },
        )
    };
    let count = |v1: &axilog_schema::v1::ReportV1| -> (usize, usize) {
        let details = v1.blocks.healing.as_ref().map_or(0, |h| {
            h.by_entity
                .0
                .values()
                .filter(|r| r.detail.is_some())
                .count()
        });
        let graphs = v1.blocks.series.as_ref().map_or(0, |s| {
            s.by_entity
                .0
                .values()
                .filter(|r| r.healing_1s.is_some())
                .count()
        });
        (details, graphs)
    };

    let (d, g) = count(&build(true, false));
    assert!(d > 0, "--skill-damage alone must still produce the detail");
    assert_eq!(g, 0, "--skill-damage alone must NOT produce the 1S graph");

    let (d, g) = count(&build(false, true));
    assert_eq!(
        d, 0,
        "--timeseries alone must NOT produce the ally matrix or the dists"
    );
    assert!(g > 0, "--timeseries alone must still produce the 1S graph");

    let (d, g) = count(&build(true, true));
    assert!(d > 0 && g > 0, "both flags produce both families");

    let (d, g) = count(&build(false, false));
    assert_eq!((d, g), (0, 0), "neither flag produces neither family");
}

/// Gate off everywhere: no detail, no graph, and the healing SCALARS are
/// still there.
///
/// The scalars are always-on -- they predate this task and ride no flag --
/// so this is what would catch the detail being absorbed by accidentally
/// making its host row conditional.
#[test]
fn the_scalars_survive_with_every_gate_off() {
    let (_enc, _metrics, _legacy, v1) = common::fixture_report_no_gates();
    let healing = v1
        .blocks
        .healing
        .as_ref()
        .expect("healing block present even with gates off");
    let series = v1.blocks.series.as_ref().expect("series block present");

    let mut rows = 0usize;
    for e in &v1.entities {
        if let Some(row) = healing.by_entity.get(e.id) {
            assert!(
                row.detail.is_none(),
                "entity {}: detail exists with the gate off",
                e.id
            );
            rows += 1;
        }
        if let Some(row) = series.by_entity.get(e.id) {
            assert!(
                row.healing_1s.is_none(),
                "entity {}: graph exists with the gate off",
                e.id
            );
        }
    }
    assert!(
        rows > 0,
        "the always-on healing scalars must survive every gate being off"
    );
}

/// The committed WvW fixture parsed to a v1 report with the healing gate
/// on, alongside the raw `HealingDetail` the block was built from.
fn healing_report_and_detail() -> (
    axilog_schema::v1::ReportV1,
    axilog_core::analysis::healing_detail::HealingDetail,
) {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/wvw-small.anon.zevtc"
    ))
    .expect("read committed fixture");
    let raw = axilog_core::evtc::decode_raw(&bytes).expect("decode fixture");
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let detail = axilog_core::analysis::healing_detail::build(&raw, &enc)
        .expect("the committed fixture carries the healing extension");
    let legacy = axilog_schema::build_report(
        &enc,
        &metrics,
        "0.0.0-test",
        None,
        None,
        true,
        true,
        false,
        None,
    );
    let passes = axilog_schema::v1::Passes {
        healing_detail: Some(&detail),
        healing_series: Some(&detail),
        ..Default::default()
    };
    let v1 = axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &legacy,
        "0.0.0-test",
        None,
        &passes,
    );
    (v1, detail)
}

/// GW2EI's `BuildHealingDist` routes an indirect (healing-over-time) row's
/// id into `buffMap` and a direct row's into `skillMap` -- checked against
/// `fixtures/local/wvw-postrework.ei.json`, where 13721 and 77020 are
/// buffs while 1066 and 53183 are skills. The block registered every row
/// as a skill regardless.
#[test]
fn indirect_heal_rows_register_as_buffs_and_direct_rows_do_not() {
    let (report_v1, detail) = healing_report_and_detail();
    let mut indirect = std::collections::BTreeSet::new();
    let mut direct = std::collections::BTreeSet::new();
    for player in &detail {
        for e in player.healing_dist.iter().chain(player.barrier_dist.iter()) {
            if e.indirect {
                indirect.insert(e.skill_id);
            } else {
                direct.insert(e.skill_id);
            }
        }
    }
    assert!(!indirect.is_empty(), "fixture must carry at least one indirect heal row");

    for id in &indirect {
        assert!(
            report_v1.catalogs.buffs.contains_key(id),
            "indirect heal id {id} must resolve in the buff catalog"
        );
    }
    for id in direct.difference(&indirect) {
        assert!(
            !report_v1.catalogs.buffs.contains_key(id)
                || axilog_core::analysis::buffs::name(*id).is_some(),
            "a purely-direct heal id {id} must not be invented as a buff"
        );
    }
}
