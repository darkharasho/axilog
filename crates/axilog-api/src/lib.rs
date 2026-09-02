//! The one place bytes become a native `ReportV1`.
//!
//! Both `axilog-node` and `axilog-cli` previously hand-rolled this
//! sequence independently; a third consumer (`arcdps-axipulse`) made the
//! drift risk concrete. The native paths now share this function; the
//! ei-json paths and `axilog-py` still carry their own sequence.

pub use axilog_core::evtc::EvtcError;
pub use axilog_schema::v1;

/// Per-call parse settings. Mirrors the Node SDK's `ParseOptions`
/// field-for-field, including `everything`'s union semantics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseOpts {
    pub replay: bool,
    pub skill_damage: bool,
    pub timeseries: bool,
    pub missiles: bool,
    pub rotation: bool,
    pub modifiers: bool,
    /// Every analysis pass this build knows about. A UNION with the
    /// individual flags, never an override -- a consumer that sets this
    /// keeps getting complete documents as later milestones add passes.
    pub everything: bool,
}

impl ParseOpts {
    pub fn everything() -> Self {
        Self { everything: true, ..Default::default() }
    }
    fn want_replay(&self) -> bool { self.replay || self.everything }
    fn want_skill_damage(&self) -> bool { self.skill_damage || self.everything }
    fn want_timeseries(&self) -> bool { self.timeseries || self.everything }
    fn want_missiles(&self) -> bool { self.missiles || self.everything }
    fn want_rotation(&self) -> bool { self.rotation || self.everything }
    fn want_modifiers(&self) -> bool { self.modifiers || self.everything }
}

/// Parse `.evtc`/`.zevtc` bytes into the native 1.0 container.
///
/// `generated_from` is the origin file NAME, never a full path -- paths
/// are environment-specific and routinely contain a user name, which the
/// PII policy scrubs.
pub fn parse_report_v1(
    bytes: &[u8],
    opts: &ParseOpts,
    generated_from: Option<&str>,
) -> Result<v1::ReportV1, axilog_core::evtc::EvtcError> {
    let want_replay = opts.want_replay();
    let want_skill_damage = opts.want_skill_damage();
    let want_timeseries = opts.want_timeseries();
    let want_missiles = opts.want_missiles();
    let want_rotation = opts.want_rotation();
    let want_modifiers = opts.want_modifiers();

    // --- body copied verbatim from axilog-node's
    // --- build_report_v1_from_bytes, minus its `.map_err(napi_err)`.
    let raw = axilog_core::evtc::decode_raw(bytes)?;
    let enc = axilog_core::model::resolve(&raw);
    let metrics = axilog_core::analysis::analyze(&enc, &raw);
    let replay = want_replay.then(|| {
        axilog_core::analysis::replay::build_replay(
            &raw,
            &enc,
            axilog_core::analysis::replay::DEFAULT_POLL_MS,
        )
    });
    let missiles = want_missiles
        .then(|| axilog_core::analysis::missiles::build_missiles(&raw, &enc));
    // Native path: whole-fight only -- the per-target split has no native
    // counterpart on this path -- it is the expensive half, and only the
    // ei-json builder below asks for it (absorption Task 13 gave it a native
    // home on `blocks.damage_mods`, but not a reason to always pay for it).
    let damage_mods = want_modifiers.then(|| {
        axilog_core::analysis::damage_mods::evaluate_catalog_full(
            &raw, &axilog_core::analysis::damage::InstidRegistry::build(&raw), &enc, false,
        )
    });
    // Side-channel absorption Task 6: these two passes were previously run
    // only on the ei-json path, so the NATIVE path emitted no `minions`
    // block and no `healthPercents` even when the caller asked for the
    // gates that produce them. They are native blocks now, so they run
    // here on the same options that gate them everywhere else.
    let minion_rollups =
        want_skill_damage.then(|| axilog_core::analysis::minions::build(&raw, &enc));
    let health_percents =
        want_timeseries.then(|| axilog_core::analysis::health::ei_health_percents(&raw, &enc));
    // Tasks 7 and 8, same story: enemy per-skill damage and the per-enemy
    // outgoing series now land on the native `damage` and `series` blocks,
    // so the native path has to run both passes too. The addr set and the
    // representative fold are shared, and built at most once.
    let enemy_sets = (want_skill_damage || want_timeseries).then(|| {
        let enemies: std::collections::BTreeSet<u64> =
            enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
        let rep: std::collections::BTreeMap<u64, u64> = enc
            .enemies
            .iter()
            .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
            .collect();
        (enemies, rep)
    });
    let enemy_dist = enemy_sets
        .as_ref()
        .filter(|_| want_skill_damage)
        .map(|(en, rep)| axilog_core::analysis::skill_damage::build_enemy_dist(&raw, en, rep));
    let enemy_series = enemy_sets.as_ref().filter(|_| want_timeseries).map(|(en, rep)| {
        axilog_core::analysis::timeseries::build_enemy_series(
            &enc,
            &raw,
            &axilog_core::analysis::damage::InstidRegistry::build(&raw),
            en,
            rep,
        )
    });
    // Task 9, same story again: the outcome columns are native now, so the
    // native path runs the pass on the gate that produces them.
    let dist_outcomes =
        want_skill_damage.then(|| axilog_core::analysis::dist_outcomes::build(&raw, &enc));
    // Task 11: ungated on purpose. `blocks.replay.by_entity` is the
    // always-on half of that block, so the native document carries
    // down/dead intervals whether or not positions were asked for.
    let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
    let replay_extras = axilog_core::analysis::replay_extras::build(&raw);
    // Task 10, the last of the same story: one pass, two families, two
    // flags -- so it runs on EITHER gate and each `Passes` field is
    // re-filtered to the flag that family actually rides.
    let healing_detail = (want_skill_damage || want_timeseries)
        .then(|| axilog_core::analysis::healing_detail::build(&raw, &enc))
        .flatten();
    // CC-strip-timelines Task 4: the per-player 1s CC/strip lanes on
    // `blocks.series.by_entity`. Gated on `--timeseries` because it is NOT
    // cheap: `build_from` derives an `InstidRegistry` (a full pass over
    // `raw.events`) and the pass itself makes several more scans on top of
    // that. Only the three address folds it also does are cheap.
    let entity_series = want_timeseries
        .then(|| axilog_core::analysis::entity_series::build_from(&enc, &raw, &metrics));
    let report = axilog_schema::build_report(
        &enc, &metrics, env!("CARGO_PKG_VERSION"), replay.as_ref(), missiles.as_ref(),
        want_skill_damage, want_timeseries, want_rotation, damage_mods.as_ref(),
    );
    // Task 12: the native path needs these too -- they feed
    // `blocks.boons`/`blocks.conditions`, not just the ei-json adapter.
    let boon_states = want_timeseries
        .then(|| axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons));
    let target_conditions =
        want_timeseries.then(|| axilog_core::analysis::target_conditions::build(&raw, &enc));
    let self_effects =
        want_timeseries.then(|| axilog_core::analysis::self_effects::build(&raw, &enc));
    // Always-on, like `activity` above: this pass emits uptime only, at
    // the cost `blocks.boons`' own always-on half already carries. Gating
    // it would empty axibridge's Special Buffs and Sigil/Relic sections on
    // every default parse.
    let squad_buffs = axilog_core::analysis::squad_buffs::build(&raw, &enc);
    // The attributed detail behind `received_cc_count` and the `cc_taken`
    // lane -- which skill landed each incoming CC, from whom, and when.
    // Gated with the lane it decomposes (see `Passes::cc_taken_events`).
    let cc_taken_events =
        want_timeseries.then(|| axilog_core::analysis::cc::taken_events_for(&enc, &raw));
    // Always-on, like `activity`/`squad_buffs` above: one linear scan of
    // `raw.events` with no per-event allocation.
    let focus = axilog_core::analysis::focus::build(&enc, &raw);
    Ok(axilog_schema::v1::build_report_v1(
        &enc,
        &metrics,
        &report,
        env!("CARGO_PKG_VERSION"),
        generated_from,
        &axilog_schema::v1::Passes {
            damage_mods: damage_mods.as_ref(),
            minions: minion_rollups.as_ref(),
            health_percents: health_percents.as_ref(),
            enemy_dist: enemy_dist.as_ref(),
            enemy_series: enemy_series.as_ref(),
            dist_outcomes: dist_outcomes.as_ref(),
            healing_detail: healing_detail.as_ref().filter(|_| want_skill_damage),
            healing_series: healing_detail.as_ref().filter(|_| want_timeseries),
            entity_series: entity_series.as_ref(),
            activity: Some(&activity),
            replay_extras: Some(&replay_extras),
            boon_states: boon_states.as_ref(),
            target_conditions: target_conditions.as_ref(),
            self_effects: self_effects.as_ref(),
            squad_buffs: Some(&squad_buffs),
            cc_taken_events: cc_taken_events.as_deref(),
            focus: Some(&focus),
        },
    ))
}
