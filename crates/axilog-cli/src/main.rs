use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "axilog", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse an arcdps .zevtc/.evtc log
    Parse {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
        /// Table-format column layout (M3, Task 5). Ignored for every other
        /// `--format` (json/csv/ei-json/html keep their existing full-field
        /// shape unconditionally).
        #[arg(long, value_enum, default_value_t = View::Default)]
        view: View,
        /// Write output to FILE instead of stdout. Applies to every
        /// `--format` (M7, Task 1).
        #[arg(short = 'o', long = "output", value_name = "FILE")]
        output: Option<PathBuf>,
        /// Compute and embed the native combat-replay block (M9, Task 2):
        /// per-squad-player and per-enemy-player-representative position
        /// tracks, downsampled to
        /// `axilog_core::analysis::replay::DEFAULT_POLL_MS`. `--format json`
        /// embeds it in the top-level `replay` field; `--format html` passes
        /// it through to the report data the client-side Replay tab reads
        /// (M9 Task 3). `--format ei-json` (M15 Task 3) instead emits
        /// GW2EI's OWN replay shape -- per-actor `combatReplayData.
        /// {positions, orientations, dc, iconURL}` in map pixels on
        /// GW2EI's fixed 300ms grid, plus the top-level
        /// `combatReplayMetaData` -- computed by the separate
        /// `axilog_core::analysis::ei_replay` engine (the two shapes differ
        /// in grid, units, intervals and rounding; see that module's doc).
        /// `--format table`/`csv` ignore it. Off by default: measured on
        /// the committed WvW fixture, the ei-json payload grows +184%
        /// pretty-printed / +142% compact.
        #[arg(long)]
        replay: bool,
        /// Compute and embed the native opt-in missile (projectile)
        /// analytics block (M10, Task 2): per-squad-player fired/hit/denied
        /// counts plus a squad-wide incoming-denied defensive rollup -- see
        /// `axilog_core::analysis::missiles` for exactly what's
        /// attributable (there is no blocked/reflected/destroyed
        /// breakdown; the wire format doesn't support one). `--format json`
        /// embeds it in the top-level `missiles` field; every other format
        /// ignores it (no comparable shape). Off by default.
        #[arg(long)]
        missiles: bool,
        /// Embed the native per-skill damage distribution block (M12 Task 1):
        /// per-squad-player outgoing damage grouped by skill id (total and
        /// per-target) and incoming damage grouped by skill id --
        /// `axilog_core::analysis::skill_damage`. Already computed
        /// unconditionally by `analyze()` regardless of this flag (cheap);
        /// this only controls whether the `Report` passed to every
        /// `--format` carries it. `--format json` embeds it in each
        /// `players[].skill_damage` field; `--format ei-json` (M12 Task 3)
        /// maps the SAME data into EI's own `totalDamageDist`/
        /// `targetDamageDist`/`totalDamageTaken` shapes (`axilog_ei::
        /// to_ei_json` keys off the `Report`'s own `skill_damage` presence,
        /// not a separate flag -- see that fn's doc comment); every other
        /// format ignores it. Off by default -- measured on the committed
        /// WvW fixture, this block alone grows the native JSON output by
        /// +249% (see `axilog_schema::Report::players`'s `PlayerOut::
        /// skill_damage`'s doc comment for the numbers), so it's opt-in
        /// like `--replay`/`--missiles` rather than always-on.
        #[arg(long)]
        skill_damage: bool,
        /// Embed the native per-player per-second series block (M12 Task 2):
        /// cumulative per-second `damage`/`damage_taken`/`per_target`, plus
        /// the per-enemy `dps_targets` summary -- `axilog_core::analysis::
        /// timeseries`. Already computed unconditionally by `analyze()`
        /// regardless of this flag (cheap); this only controls whether the
        /// `Report` passed to every `--format` carries it. `--format json`
        /// embeds `players[].per_second` AND `players[].dps_targets`;
        /// `--format ei-json` (M12 Task 3) maps the SAME data into EI's own
        /// `damage1S`/`targetDamage1S`/`damageTaken1S`/`dpsTargets` shapes
        /// (`axilog_ei::to_ei_json` keys off the `Report`'s own
        /// `per_second` presence, not a separate flag); every other format
        /// ignores both. Off by default -- measured on the committed WvW
        /// fixture, `per_second` alone grows
        /// the native JSON output by +147.7% and `dps_targets` alone by
        /// +36.4% (both individually past the ~30% size-discipline
        /// guideline -- `dps_targets` is NOT small on a real WvW zerg log,
        /// which can enumerate dozens of enemy players/siege/dolyaks/guards
        /// per player), so BOTH stay gated behind this one flag, same
        /// opt-in reasoning as `--skill-damage`. See `axilog_schema::
        /// Report::players`'s `PlayerOut::per_second`/`PlayerOut::
        /// dps_targets` doc comments for the full numbers.
        #[arg(long)]
        timeseries: bool,
        /// Embed the native per-player rotation (cast tracking) block (M14
        /// Task 1): per-squad-player cast list grouped by skill id --
        /// `axilog_core::analysis::rotation`. Already computed
        /// unconditionally by `analyze()` regardless of this flag (cheap);
        /// this only controls whether the `Report` passed to every
        /// `--format` carries it. `--format json` embeds it in each
        /// `players[].rotation` field; every other format ignores it. Off
        /// by default -- measured on the committed WvW fixture (compact
        /// JSON): 170,451 -> 284,535 bytes (+66.9%), well past the ~30%
        /// size-discipline guideline (see `axilog_schema::Report::players`'s
        /// `PlayerOut::rotation` doc comment for the full writeup), so
        /// it's opt-in like `--skill-damage`/`--timeseries`.
        #[arg(long)]
        rotation: bool,
        /// Embed the per-player damage-modifier stats (M16): for every
        /// trait/rune/relic/food modifier a player actually triggered, how
        /// many of the eligible hits it applied to and how much of the
        /// damage it is responsible for --
        /// `axilog_core::analysis::damage_mods`.
        ///
        /// `--format json` embeds `players[].damage_mods.{outgoing,
        /// incoming}` plus the top-level `damage_mod_map`.
        /// `--format html` carries the SAME native block, because the
        /// report page embeds the serialized `Report` verbatim
        /// (`axilog_html::render`) -- it grows 260,515 -> 347,412 bytes on
        /// the committed fixture, exactly as `--rotation`/`--skill-damage`/
        /// `--timeseries` already do; there is no modifier-specific HTML
        /// widget, and the flagless page is unchanged (both HTML size
        /// budgets are measured flagless and are untouched).
        /// `--format ei-json` embeds Elite Insights' own
        /// `damageModifiers`/`incomingDamageModifiers`/
        /// `damageModifiersTarget`/`incomingDamageModifiersTarget` plus
        /// `damageModMap` -- gated by this same flag, and additionally the
        /// only format that asks the engine for the per-target split.
        /// `--format table`/`csv` ignore it entirely.
        ///
        /// Off by default, and unlike `--rotation`/`--skill-damage`/
        /// `--timeseries` this flag gates the COMPUTATION, not just the
        /// serialization: `analyze()` does not run the modifier engine, so
        /// nothing pays for it unless asked. The engine is a separate pass
        /// over every damage event crossed with ~200 catalogued
        /// definitions, plus a per-`(actor, buff)` stack-timeline
        /// simulation.
        ///
        /// Measured on the committed WvW fixture (compact JSON, 42
        /// players, 80 targets): `--format json` 194,773 -> 280,843 bytes
        /// (+44.2%); `--format ei-json` 216,173 -> 1,170,570 bytes
        /// (+441.5%). The gap is EI's per-target arrays, which have no
        /// native counterpart and are 854,077 of those bytes on their own
        /// (`damageModifiersTarget` 497,702 + its incoming twin 356,375)
        /// -- see `axilog_ei::EiInputs::modifiers`. Wall clock on that
        /// fixture (release build, `--format ei-json`): 0.074s -> 0.155s.
        #[arg(long)]
        modifiers: bool,
    },
    /// Rewrite every player's character/account name in a .zevtc to a
    /// deterministic `Anon<N>` placeholder and write the result as a new
    /// .zevtc. All other bytes (including every combat event) are
    /// preserved byte-for-byte, so analysis output is identical to the
    /// original — useful for producing PII-safe fixtures for bug reports,
    /// sharing logs, or committing test fixtures.
    Anonymize { input: PathBuf, output: PathBuf },
}

#[derive(Copy, Clone, PartialEq, ValueEnum)]
enum Format {
    Json,
    Table,
    Csv,
    EiJson,
    /// Self-contained dark-theme HTML report (M7, Task 1) — see
    /// `axilog-html`. No external requests; CSS/JS/data are all inlined.
    Html,
}

/// Table-format column layout (M3, Task 5).
#[derive(Copy, Clone, PartialEq, ValueEnum)]
enum View {
    /// Damage/downs/kills/deaths — unchanged from Task 14.
    Default,
    /// Condi cleanses / boon strips / resurrects / stun breaks.
    Support,
    /// Might average stacks + presence % for Quickness/Alacrity/Stability/
    /// Protection.
    Boons,
    /// arcdps healing-extension totals (M10, Task 1): healing out (total),
    /// allies, barrier out, downed-ally healing.
    Healing,
    /// Incoming defenses (M13, Task 3): blocks/evades/dodges, total damage
    /// taken (+ strike/condi split), downs taken.
    Defense,
    /// Per-player rotation summary (M14, Task 3): total animated-cast count
    /// (`axilog_core::analysis::rotation::total_casts`) plus APM (Actions
    /// Per Minute, `casts / (active_ms / 60_000)`, using M11's
    /// `ActivityIntervals::active_ms` as the active-time denominator).
    /// Reads `axilog_core::analysis::Metrics::players[].rotation` DIRECTLY
    /// (via the `metrics`/`activity` this table-rendering path already has
    /// in scope), NOT `Report::players[].rotation` -- unlike every other
    /// view above, this one does NOT require `--rotation` to have also been
    /// passed, since `PlayerMetrics::rotation` is always computed by
    /// `analyze()` regardless of that flag (that flag only gates whether
    /// the native/ei-json JSON payload carries the full per-cast detail --
    /// see `axilog_schema::PlayerOut::rotation`'s doc comment).
    Rotation,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Parse { path, format, view, output, replay, missiles, skill_damage, timeseries, rotation, modifiers } => {
            let bytes = std::fs::read(&path)?;
            let raw = axilog_core::evtc::decode_raw(&bytes)?;
            let enc = axilog_core::model::resolve(&raw);
            let metrics = axilog_core::analysis::analyze(&enc, &raw);
            let replay_data = replay.then(|| {
                axilog_core::analysis::replay::build_replay(
                    &raw,
                    &enc,
                    axilog_core::analysis::replay::DEFAULT_POLL_MS,
                )
            });
            let missiles_data =
                missiles.then(|| axilog_core::analysis::missiles::build_missiles(&raw, &enc));
            // M15 Task 3: `--replay` additionally turns on GW2EI's own
            // fixed-rate replay shape for `--format ei-json`
            // (`combatReplayData.{positions, orientations, dc, iconURL}` +
            // the top-level `combatReplayMetaData`). It is a SECOND engine
            // over the same events, not a reshaping of `replay_data` above
            // -- see `axilog_core::analysis::ei_replay`'s module doc for
            // why the two shapes are deliberately separate -- so it is
            // computed only for the format that can emit it.
            let ei_replay_data = (replay && format == Format::EiJson)
                .then(|| axilog_core::analysis::ei_replay::build_ei_replay_auto(&raw, &enc));
            // M11 Task 3: down/dead intervals + activeTimes are cheap (no
            // position decode/downsample), so they're computed
            // unconditionally -- unlike `--replay`'s `replay_data` above,
            // which stays opt-in. Originally only threaded into the
            // `ei-json` branch below; M14 Task 3's `--view rotation` (APM)
            // needs the same data for the `table` branch too, so this is
            // hoisted out here and shared by both rather than computed
            // twice.
            let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
            // MEIGAP Task 1b: GW2EI-shape boon stack timelines
            // (`buffUptimes[].states`/`.statesPerSource`) -- gated on
            // `--timeseries`, mirroring GW2EI's own `RawFormatTimelineArrays`
            // gate on the same two arrays. It re-runs the boon simulation to
            // recover per-SOURCE stack ownership, which `analyze()` keeps
            // only in summed form -- see
            // `axilog_core::analysis::buffs::states`'s module doc.
            //
            // Task 12: no longer restricted to `Format::EiJson`. It lands on
            // `blocks.boons`' rows now, so `--format json --timeseries` is
            // entitled to it too -- the flag alone decides, exactly as
            // Tasks 7 and 8 did for the enemy passes below.
            let boon_states = timeseries.then(|| {
                axilog_core::analysis::buffs::states::build(&raw, &enc, &metrics.boons)
            });
            // MEIGAP Task 2b/2d: the two `--timeseries`-gated `targets[]`
            // mirrors, on exactly the same gate and for the same reason
            // (GW2EI puts `targets[].damage1S`/`.powerDamage1S` behind
            // `RawFormatTimelineArrays` at `JsonActorBuilder.cs:63`, and
            // `statesPerSource` behind it at `JsonBuffsUptimeBuilder.cs:52`).
            // Both are standalone passes, not part of `analyze()`, so a
            // flagless parse pays nothing for them.
            // The enemy addr set + representative fold both enemy-side
            // passes need -- built at most once, and only when one of them
            // will actually run (a flagless parse pays nothing).
            // Side-channel absorption Tasks 7 and 8: both enemy passes now
            // land on native blocks (`blocks.damage` and `blocks.series`),
            // so neither half of this gate depends on the output format any
            // more -- the flags alone decide.
            let enemy_sets = (skill_damage || timeseries).then(|| {
                let enemies: std::collections::BTreeSet<u64> =
                    enc.enemies.iter().flat_map(|e| e.agent_addrs.iter().copied()).collect();
                let enemy_addr_to_rep: std::collections::BTreeMap<u64, u64> = enc
                    .enemies
                    .iter()
                    .flat_map(|e| e.agent_addrs.iter().map(move |&a| (a, e.id)))
                    .collect();
                (enemies, enemy_addr_to_rep)
            });
            let enemy_series = enemy_sets.as_ref().filter(|_| timeseries).map(|(en, rep)| {
                axilog_core::analysis::timeseries::build_enemy_series(
                    &enc,
                    &raw,
                    &axilog_core::analysis::damage::InstidRegistry::build(&raw),
                    en,
                    rep,
                )
            });
            // Task 12: lands on `blocks.conditions`, so likewise no longer
            // format-restricted.
            let target_conditions =
                timeseries.then(|| axilog_core::analysis::target_conditions::build(&raw, &enc));
            // MEIGAP Task 2c: `targets[].totalDamageDist` rides
            // `--skill-damage`, the flag that already gates every other
            // per-skill block (GW2EI itself emits it unconditionally; this
            // is a payload gate, and axibridge hardcodes the flag on).
            let enemy_dist = enemy_sets.as_ref().filter(|_| skill_damage).map(|(en, rep)| {
                axilog_core::analysis::skill_damage::build_enemy_dist(&raw, en, rep)
            });
            // MEIGAP Task 3a/3b. Every healing-detail family is
            // flag-gated -- `healing1S` on `--timeseries`, the ally
            // matrices and the two `*Dist` arrays on `--skill-damage` (see
            // `Passes::healing_series` for the measured payload reason) --
            // so the PASS itself only runs when at least one of them will
            // be serialized, and it self-gates to `None` on a log with no
            // healing extension before it even builds a registry.
            // `minions[]` is a per-skill distribution and rides
            // `--skill-damage` outright.
            //
            // Side-channel absorption Task 10 dropped the `&& format ==
            // Format::EiJson`, for the same reason Tasks 6 and 9 dropped
            // it: both halves are native surfaces now
            // (`blocks.healing.by_entity[].detail` and
            // `blocks.series.by_entity[].healing_1s`), so the flags mean
            // the same thing whichever format is being written. The two
            // `Passes` fields below are what re-split it: one pass, two
            // families, two flags.
            let healing_detail = (skill_damage || timeseries)
                .then(|| axilog_core::analysis::healing_detail::build(&raw, &enc))
                .flatten();
            // Side-channel absorption Task 6: no longer `&& format ==
            // Format::EiJson`. `minions` is now a native block, so the
            // pass runs for whatever `--skill-damage` was asked for,
            // regardless of which format is being written.
            let minion_rollups =
                skill_damage.then(|| axilog_core::analysis::minions::build(&raw, &enc));
            // MEIGAP2 row 1: the player-side distributions' outcome columns
            // ride `--skill-damage`, the same gate as the distributions they
            // annotate -- so this pass cannot run for output that would not
            // carry it. See `axilog_core::analysis::dist_outcomes`'s module
            // doc for why it is a standalone pass rather than more work
            // inside `analyze()`.
            // Side-channel absorption Task 9: no longer `&& format ==
            // Format::EiJson`, for the same reason `minion_rollups` above
            // dropped it in Task 6 -- these columns are now a native
            // surface (`blocks.damage.by_entity[].by_skill[].outcomes`),
            // so gating them on the output format would make the native
            // JSON's contents depend on which writer was asked for.
            let dist_outcomes =
                skill_damage.then(|| axilog_core::analysis::dist_outcomes::build(&raw, &enc));
            // MEIGAP2 row 2: `players[].healthPercents` rides
            // `--timeseries`, GW2EI's own `RawFormatTimelineArrays` gate on
            // that field.
            // Task 6, as above: `blocks.series` carries these natively now.
            let health_percents =
                timeseries.then(|| axilog_core::analysis::health::ei_health_percents(&raw, &enc));
            // M16: the damage-modifier engine runs ONLY on `--modifiers`
            // (see the flag's doc comment -- it is a separate full event
            // pass, not a copy of something `analyze()` already computed).
            // The per-target split is asked for only by `ei-json`, the one
            // format with a shape for it (`damageModifiersTarget`); it is
            // the expensive half, so the native path skips it.
            let damage_mods = modifiers.then(|| {
                axilog_core::analysis::damage_mods::evaluate_catalog_full(
                    &raw,
                    &axilog_core::analysis::damage::InstidRegistry::build(&raw),
                    &enc,
                    format == Format::EiJson,
                )
            });
            let report = axilog_schema::build_report(
                &enc,
                &metrics,
                env!("CARGO_PKG_VERSION"),
                replay_data.as_ref(),
                missiles_data.as_ref(),
                skill_damage,
                timeseries,
                rotation,
                damage_mods.as_ref(),
            );
            // Hoisted above the `format == Format::EiJson` branch (side-
            // channel absorption, Task 3) so both the `ei-json` streaming
            // path and the `json` arm below share the one `ReportV1` build
            // instead of constructing it twice.
            let report_v1 = axilog_schema::v1::build_report_v1(
                &enc,
                &metrics,
                &report,
                env!("CARGO_PKG_VERSION"),
                path.file_name().and_then(|s| s.to_str()),
                &axilog_schema::v1::Passes {
                    damage_mods: damage_mods.as_ref(),
                    minions: minion_rollups.as_ref(),
                    health_percents: health_percents.as_ref(),
                    enemy_dist: enemy_dist.as_ref(),
                    enemy_series: enemy_series.as_ref(),
                    dist_outcomes: dist_outcomes.as_ref(),
                    healing_detail: healing_detail.as_ref().filter(|_| skill_damage),
                    healing_series: healing_detail.as_ref().filter(|_| timeseries),
                    activity: Some(&activity),
                    boon_states: boon_states.as_ref(),
                    target_conditions: target_conditions.as_ref(),
                },
            );
            // Final-review fix wave: surface analysis warnings (e.g. a
            // post-2026-05-01 buff-statechange-rework build producing
            // all-zero boon/support metrics) on stderr for every output
            // format -- `json`/`ei-json` still emit the metrics themselves
            // unchanged (json additionally carries them in the
            // `warnings: [...]` top-level field; ei-json has no comparable
            // field and omits them).
            for w in &report.warnings {
                eprintln!("warning: {w}");
            }
            // `ei-json` is the one format that does NOT render to a `String`
            // first (MSTREAM). It is by far the largest output this CLI
            // produces -- on a 583k-event real log with every flag on, the
            // pre-MSTREAM path peaked at ~1.28 GB RSS holding the whole
            // `serde_json::Value` tree AND its pretty-printed `String` at the
            // same time. `axilog_ei::write_ei_json` streams the document row
            // by row into a `BufWriter`, so nothing bigger than one player
            // row is ever resident. Output is byte-identical to what the old
            // `to_string_pretty(&to_ei_json(..))` produced, trailing newline
            // included (see that function's doc comment, and axilog-ei's
            // `streaming_matches_value_tree_byte_for_byte` test).
            if format == Format::EiJson {
                let ei_inputs = axilog_ei::EiInputs {
                    replay: ei_replay_data.as_ref(),
                    modifiers: damage_mods.as_ref(),
                };
                // One `dyn Write` so the two destinations share the emit
                // code; the `BufWriter` (not the trait object) is what makes
                // the per-token writes cheap.
                let sink: Box<dyn std::io::Write> = match &output {
                    Some(path) => Box::new(std::fs::File::create(path)?),
                    None => Box::new(std::io::stdout().lock()),
                };
                use std::io::Write as _;
                let mut w = std::io::BufWriter::with_capacity(1 << 20, sink);
                axilog_ei::write_ei_json(&report_v1, &report, &ei_inputs, &mut w)?;
                w.write_all(b"\n")?;
                // Explicit flush: a `BufWriter`'s `Drop` swallows write
                // errors (a full disk would otherwise truncate silently).
                w.flush()?;
                drop(w);
                if let Some(path) = &output {
                    eprintln!("wrote {}", path.display());
                }
                return Ok(());
            }
            // Every other format renders to a single `String` (with its own
            // trailing newline where appropriate) so `-o/--output` (M7,
            // Task 1) can apply uniformly regardless of `--format`.
            let rendered = match format {
                Format::Json => {
                    format!("{}\n", serde_json::to_string_pretty(&report_v1)?)
                }
                Format::EiJson => unreachable!("handled by the streaming path above"),
                Format::Table => axilog_cli_table(&report, view, &metrics, &activity),
                Format::Csv => axilog_cli_csv(&report),
                Format::Html => axilog_html::render(&report),
            };
            match output {
                Some(path) => {
                    std::fs::write(&path, &rendered)?;
                    eprintln!("wrote {}", path.display());
                }
                None => print!("{rendered}"),
            }
        }
        Cmd::Anonymize { input, output } => {
            let bytes = std::fs::read(&input)?;
            let mut data = axilog_core::evtc::inflate_zevtc(&bytes)?;
            let n = axilog_core::evtc::anonymize_raw_evtc(&mut data)?;
            let entry_name = output
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("log");
            let zipped = axilog_core::evtc::zip_deflate(&format!("{entry_name}.evtc"), &data);
            std::fs::write(&output, zipped)?;
            eprintln!("anonymized {n} player agent(s): {} -> {}", input.display(), output.display());
        }
    }
    Ok(())
}

// table/csv helpers added in Task 14; `view` (M3, Task 5) selects the
// column layout below. `metrics`/`activity` (M14, Task 3) are only used by
// `View::Rotation` (see that variant's doc comment for why it reads
// `Metrics` directly rather than `Report`) -- every other view ignores
// them.
fn axilog_cli_table(
    r: &axilog_schema::Report,
    view: View,
    metrics: &axilog_core::analysis::Metrics,
    activity: &[axilog_core::analysis::replay::ActivityIntervals],
) -> String {
    match view {
        View::Default => axilog_cli_table_default(r),
        View::Support => axilog_cli_table_support(r),
        View::Boons => axilog_cli_table_boons(r),
        View::Healing => axilog_cli_table_healing(r),
        View::Defense => axilog_cli_table_defense(r),
        View::Rotation => axilog_cli_table_rotation(r, metrics, activity),
    }
}
fn axilog_cli_table_default(r: &axilog_schema::Report) -> String {
    let mut s = String::new();
    s.push_str(&format!("{:<24} {:<12} {:>10} {:>8} {:>6} {:>6} {:>7}\n",
        "account", "profession", "damage", "DPS", "downs", "kills", "deaths"));
    let mut players: Vec<_> = r.players.iter().collect();
    players.sort_by(|a, b| b.damage.total.cmp(&a.damage.total));
    for p in players {
        s.push_str(&format!("{:<24} {:<12} {:>10} {:>8.0} {:>6} {:>6} {:>7}\n",
            trunc(&p.account, 24), trunc(&p.profession, 12), p.damage.total,
            p.damage.dps, p.downs_dealt, p.kills_dealt, p.deaths));
    }
    s
}
/// `--view support`: condi cleanses / boon strips / resurrects / stun
/// breaks (M3, Task 5 brief). `cleanses` here is `SupportOut.cleanses`
/// (friendly-other cleanses, not `cleanses_self` — matching the brief's
/// plain "cleanses" column name).
fn axilog_cli_table_support(r: &axilog_schema::Report) -> String {
    let mut s = String::new();
    s.push_str(&format!("{:<24} {:<12} {:>9} {:>7} {:>10} {:>10}\n",
        "account", "profession", "cleanses", "strips", "resurrects", "stunbreaks"));
    let mut players: Vec<_> = r.players.iter().collect();
    players.sort_by_key(|p| std::cmp::Reverse(p.support.cleanses));
    for p in players {
        s.push_str(&format!("{:<24} {:<12} {:>9} {:>7} {:>10} {:>10}\n",
            trunc(&p.account, 24), trunc(&p.profession, 12), p.support.cleanses,
            p.support.strips, p.support.resurrects, p.cc.stun_breaks));
    }
    s
}
/// `--view boons`: Might average stacks (its Intensity-type headline
/// number, `avg_stacks`) plus presence % for the other three "key" boons
/// the brief names — Quickness/Alacrity/Stability/Protection (M3, Task 5
/// brief). Stability's `presence_pct` (not its own `avg_stacks`) is shown
/// here to keep every non-Might column on the same 0-100% scale.
fn axilog_cli_table_boons(r: &axilog_schema::Report) -> String {
    let mut s = String::new();
    s.push_str(&format!("{:<24} {:<12} {:>10} {:>7} {:>7} {:>7} {:>7}\n",
        "account", "profession", "Might(avg)", "Quick%", "Alac%", "Stab%", "Prot%"));
    let mut players: Vec<_> = r.players.iter().collect();
    players.sort_by(|a, b| a.account.cmp(&b.account));
    for p in players {
        let find = |id: u32| p.boons.iter().find(|b| b.id == id);
        let might_avg = find(axilog_core::analysis::buffs::MIGHT).and_then(|b| b.avg_stacks).unwrap_or(0.0);
        let quick = find(axilog_core::analysis::buffs::QUICKNESS).map(|b| b.presence_pct).unwrap_or(0.0);
        let alac = find(axilog_core::analysis::buffs::ALACRITY).map(|b| b.presence_pct).unwrap_or(0.0);
        let stab = find(axilog_core::analysis::buffs::STABILITY).map(|b| b.presence_pct).unwrap_or(0.0);
        let prot = find(axilog_core::analysis::buffs::PROTECTION).map(|b| b.presence_pct).unwrap_or(0.0);
        s.push_str(&format!("{:<24} {:<12} {:>10.2} {:>7.1} {:>7.1} {:>7.1} {:>7.1}\n",
            trunc(&p.account, 24), trunc(&p.profession, 12), might_avg, quick, alac, stab, prot));
    }
    s
}
/// `--view healing` (M10, Task 1): arcdps healing-extension totals --
/// account, profession, healing out (total, self+allies), allies (healing
/// out excluding self), barrier out, downed-ally healing. When the log
/// carries no healing-extension data at all (`p.healing` is `None` for
/// every player -- `axilog_schema::PlayerOut.healing`'s doc comment), every
/// row renders as `-` rather than a misleading `0` (the caller's `main`
/// already prints the matching "healing extension not present" warning to
/// stderr for every `--format`, this table just avoids implying real zero
/// data on top of that).
fn axilog_cli_table_healing(r: &axilog_schema::Report) -> String {
    let mut s = String::new();
    s.push_str(&format!("{:<24} {:<12} {:>12} {:>12} {:>10} {:>10}\n",
        "account", "profession", "healing out", "allies", "barrier", "downed"));
    let has_any = r.players.iter().any(|p| p.healing.is_some());
    let mut players: Vec<_> = r.players.iter().collect();
    players.sort_by_key(|p| std::cmp::Reverse(p.healing.as_ref().map(|h| h.healing_out_total).unwrap_or(0)));
    for p in players {
        match &p.healing {
            Some(h) => s.push_str(&format!("{:<24} {:<12} {:>12} {:>12} {:>10} {:>10}\n",
                trunc(&p.account, 24), trunc(&p.profession, 12),
                h.healing_out_total, h.healing_out_allies, h.barrier_out, h.downed_healing_out)),
            None => s.push_str(&format!("{:<24} {:<12} {:>12} {:>12} {:>10} {:>10}\n",
                trunc(&p.account, 24), trunc(&p.profession, 12), "-", "-", "-", "-")),
        }
    }
    if !has_any {
        s.push_str("(healing extension not present in this log)\n");
    }
    s
}
/// `--view defense` (M13, Task 3): incoming defenses -- account,
/// profession, blocks/evades/dodges (hit-outcome counts, `p.defenses`),
/// total damage taken (`p.damage_taken`, the pre-existing whole-fight
/// scalar -- NOT `p.defenses.strike_damage + condition_damage +
/// life_leech_damage`, which excludes barrier/blocked/evaded/etc. and is a
/// narrower, additive breakdown, not a replacement total -- see
/// `axilog_core::analysis::defenses`'s module doc), a strike/condi split
/// (`p.defenses.strike_damage`/`condition_damage`, the brief's "maybe"
/// extra columns) alongside it, and downs taken (`p.downs_taken`).
/// `p.defenses` is always present (not gated), so this view never needs the
/// healing view's "extension not present" fallback dashes.
fn axilog_cli_table_defense(r: &axilog_schema::Report) -> String {
    let mut s = String::new();
    s.push_str(&format!("{:<24} {:<12} {:>7} {:>7} {:>7} {:>10} {:>9} {:>9} {:>6}\n",
        "account", "profession", "blocks", "evades", "dodges", "dmg taken", "strike", "condi", "downs"));
    let mut players: Vec<_> = r.players.iter().collect();
    players.sort_by_key(|p| std::cmp::Reverse(p.damage_taken));
    for p in players {
        s.push_str(&format!("{:<24} {:<12} {:>7} {:>7} {:>7} {:>10} {:>9} {:>9} {:>6}\n",
            trunc(&p.account, 24), trunc(&p.profession, 12),
            p.defenses.blocked_count, p.defenses.evaded_count, p.defenses.dodge_count,
            p.damage_taken, p.defenses.strike_damage, p.defenses.condition_damage,
            p.downs_taken));
    }
    s
}
/// `--view rotation` (M14, Task 3): per-player total animated-cast count
/// (`axilog_core::analysis::rotation::total_casts`, summed across every
/// skill id in `PlayerMetrics::rotation`) plus APM (Actions Per Minute --
/// `casts / (active_ms / 60_000.0)`, using M11's `ActivityIntervals::
/// active_ms` as the active-time denominator, not raw fight duration --
/// mirrors how EI's own "per-minute" derived stats scale off active time,
/// not wall-clock duration). `r.players`/`metrics.players`/`activity` are
/// positionally joined -- all three are built by iterating `enc.players` in
/// the same order (`axilog_schema::build_report`'s player loop,
/// `axilog_core::analysis::analyze`'s own `enc.players.iter().map(..)`
/// player-list construction, and `build_activity_intervals`'s doc comment,
/// respectively -- the SAME positional-join convention `axilog_ei::
/// to_ei_json`'s own `player_idx` join for `activeTimes`/`combatReplayData`
/// already establishes). Reads `PlayerMetrics::rotation` DIRECTLY rather
/// than `PlayerOut::rotation` -- see `View::Rotation`'s own doc comment for
/// why this view doesn't need `--rotation` to have also been passed.
fn axilog_cli_table_rotation(
    r: &axilog_schema::Report,
    metrics: &axilog_core::analysis::Metrics,
    activity: &[axilog_core::analysis::replay::ActivityIntervals],
) -> String {
    let mut s = String::new();
    s.push_str(&format!("{:<24} {:<12} {:>8} {:>8}\n", "account", "profession", "casts", "APM"));
    let mut rows: Vec<(String, String, usize, f64)> = r
        .players
        .iter()
        .zip(metrics.players.iter())
        .zip(activity.iter())
        .map(|((p, pm), act)| {
            let casts = axilog_core::analysis::rotation::total_casts(&pm.rotation);
            let active_secs = act.active_ms() as f64 / 1000.0;
            let apm = if active_secs > 0.0 { casts as f64 / (active_secs / 60.0) } else { 0.0 };
            (p.account.clone(), p.profession.clone(), casts, apm)
        })
        .collect();
    rows.sort_by_key(|(_, _, casts, _)| std::cmp::Reverse(*casts));
    for (account, profession, casts, apm) in rows {
        s.push_str(&format!("{:<24} {:<12} {:>8} {:>8.1}\n",
            trunc(&account, 24), trunc(&profession, 12), casts, apm));
    }
    s
}
/// M11 Task 2 fix round: this column carries `downs_contribution.damage` --
/// the arcdps-methodology `damage_to_downs` value (see `axilog_core::
/// analysis::contribution`'s module doc), NOT the retired M1-era 10s-window
/// approximation. The column header is named `damage_to_downs` (renamed
/// from the original `down_contribution`, review fix round 1) rather than
/// keeping the old name on new semantics -- CSV has no schema-version
/// field, so a consumer reading an unchanged column name would silently get
/// different numbers with no way to detect the change.
fn axilog_cli_csv(r: &axilog_schema::Report) -> String {
    let mut s = String::from("account,character,profession,team,damage,dps,downs_dealt,kills_dealt,damage_to_downs,deaths\n");
    for p in &r.players {
        s.push_str(&format!("{},{},{},{},{},{:.0},{},{},{},{}\n",
            p.account, p.character, p.profession, p.team, p.damage.total, p.damage.dps,
            p.downs_dealt, p.kills_dealt, p.downs_contribution.damage, p.deaths));
    }
    s
}
fn trunc(s: &str, n: usize) -> String { s.chars().take(n).collect() }
