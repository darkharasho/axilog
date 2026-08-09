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
        /// (M9 Task 3). Every other format ignores it (no comparable
        /// shape). Off by default.
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
    },
    /// Rewrite every player's character/account name in a .zevtc to a
    /// deterministic `Anon<N>` placeholder and write the result as a new
    /// .zevtc. All other bytes (including every combat event) are
    /// preserved byte-for-byte, so analysis output is identical to the
    /// original — useful for producing PII-safe fixtures for bug reports,
    /// sharing logs, or committing test fixtures.
    Anonymize { input: PathBuf, output: PathBuf },
}

#[derive(Copy, Clone, ValueEnum)]
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Parse { path, format, view, output, replay, missiles, skill_damage, timeseries } => {
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
            let report = axilog_schema::build_report(
                &enc,
                &metrics,
                env!("CARGO_PKG_VERSION"),
                replay_data.as_ref(),
                missiles_data.as_ref(),
                skill_damage,
                timeseries,
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
            // Every format renders to a single `String` (with its own
            // trailing newline where appropriate) so `-o/--output` (M7,
            // Task 1) can apply uniformly regardless of `--format`.
            let rendered = match format {
                Format::Json => format!("{}\n", serde_json::to_string_pretty(&report)?),
                Format::EiJson => {
                    // M11 Task 3: down/dead intervals + activeTimes are
                    // cheap (no position decode/downsample), so they're
                    // computed here unconditionally -- unlike `--replay`'s
                    // `replay_data` above, which stays opt-in.
                    let activity = axilog_core::analysis::replay::build_activity_intervals(&raw, &enc);
                    format!(
                        "{}\n",
                        serde_json::to_string_pretty(&axilog_ei::to_ei_json(&report, &activity))?
                    )
                }
                Format::Table => axilog_cli_table(&report, view),
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
// column layout below.
fn axilog_cli_table(r: &axilog_schema::Report, view: View) -> String {
    match view {
        View::Default => axilog_cli_table_default(r),
        View::Support => axilog_cli_table_support(r),
        View::Boons => axilog_cli_table_boons(r),
        View::Healing => axilog_cli_table_healing(r),
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
