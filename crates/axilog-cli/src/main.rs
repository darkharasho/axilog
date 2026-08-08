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
    },
}

#[derive(Copy, Clone, ValueEnum)]
enum Format {
    Json,
    Table,
    Csv,
    EiJson,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Parse { path, format } => {
            let bytes = std::fs::read(&path)?;
            let raw = axilog_core::evtc::decode_raw(&bytes)?;
            let enc = axilog_core::model::resolve(&raw);
            let metrics = axilog_core::analysis::analyze(&enc, &raw);
            let report = axilog_schema::build_report(&enc, &metrics, env!("CARGO_PKG_VERSION"));
            match format {
                Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
                Format::EiJson => println!(
                    "{}",
                    serde_json::to_string_pretty(&axilog_ei::to_ei_json(&report))?
                ),
                Format::Table => print!("{}", axilog_cli_table(&report)),
                Format::Csv => print!("{}", axilog_cli_csv(&report)),
            }
        }
    }
    Ok(())
}

// table/csv helpers added in Task 14:
fn axilog_cli_table(r: &axilog_schema::Report) -> String {
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
fn axilog_cli_csv(r: &axilog_schema::Report) -> String {
    let mut s = String::from("account,character,profession,team,damage,dps,downs_dealt,kills_dealt,down_contribution,deaths\n");
    for p in &r.players {
        s.push_str(&format!("{},{},{},{},{},{:.0},{},{},{},{}\n",
            p.account, p.character, p.profession, p.team, p.damage.total, p.damage.dps,
            p.downs_dealt, p.kills_dealt, p.down_contribution, p.deaths));
    }
    s
}
fn trunc(s: &str, n: usize) -> String { s.chars().take(n).collect() }
