//! anchor — CLI entry point.
//!
//! Run `anchor plan` to compare the declared roots manifest against the live
//! watchman state and print a reconcile plan.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use anchor::{Error, FakeBackend, RootsConfig, WatchBackend, WatchState, WatchmanBackend, reconcile};

/// Declare which watch roots should be live, then diff against reality.
#[derive(Debug, Parser)]
#[command(name = "anchor", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Show a reconcile plan: declared-vs-live diff.
    Plan(PlanArgs),
}

#[derive(Debug, clap::Args)]
struct PlanArgs {
    /// Path to the roots manifest (default: ~/.config/anchor/roots.toml).
    #[arg(long)]
    manifest: Option<PathBuf>,

    /// Output format.
    #[arg(long, default_value = "table")]
    format: OutputFormat,

    /// Use a fake empty backend instead of real watchman (for CI/testing).
    #[arg(long, hide = true)]
    fake_backend: bool,
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    Table,
    Json,
}

fn default_manifest_path() -> PathBuf {
    let mut p = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("/home/jsy"));
    p.push(".config/anchor/roots.toml");
    p
}

/// Convert `Duration::as_secs()` (u64) to i64 for use as Unix timestamp.
/// Saturates to `i64::MAX` on overflow (won't happen until year ~292 billion).
fn duration_secs_to_i64(d: std::time::Duration) -> i64 {
    i64::try_from(d.as_secs()).unwrap_or(i64::MAX)
}

fn run() -> Result<ExitCode, Error> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Plan(args) => {
            let manifest_path = args.manifest.unwrap_or_else(default_manifest_path);
            let cfg = RootsConfig::load(&manifest_path)?;

            let live: Vec<WatchState> = if args.fake_backend {
                FakeBackend { live: vec![] }.live_roots()?
            } else {
                WatchmanBackend.live_roots()?
            };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(duration_secs_to_i64)
                .unwrap_or(0);

            let plan = reconcile(&cfg.roots, &live, now);

            match args.format {
                OutputFormat::Json => {
                    let json = serde_json::to_string_pretty(&plan)
                        .map_err(|e| Error::Watchman(format!("json serialize: {e}")))?;
                    println!("{json}");
                }
                OutputFormat::Table => {
                    print_table(&plan);
                }
            }

            if plan.missing_count > 0 {
                return Ok(ExitCode::FAILURE);
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

const fn status_str(entry: &anchor::ReconcileEntry) -> &'static str {
    match &entry.status {
        anchor::RootStatus::Watched => "watched",
        anchor::RootStatus::Missing => "MISSING",
        anchor::RootStatus::Stale { .. } => "stale",
        anchor::RootStatus::Undeclared => "undeclared",
    }
}

const fn action_str(entry: &anchor::ReconcileEntry) -> &'static str {
    match &entry.action {
        anchor::ReconcileAction::Watch { .. } => "watch",
        anchor::ReconcileAction::ReseedCursor { .. } => "reseed-cursor",
        anchor::ReconcileAction::NoOp { .. } => "no-op",
        anchor::ReconcileAction::NoteUndeclared { .. } => "note-undeclared",
    }
}

fn print_table(plan: &anchor::ReconcilePlan) {
    use tabled::{Table, Tabled, settings::Style};

    #[derive(Tabled)]
    struct Row {
        #[tabled(rename = "Root")]
        root: String,
        #[tabled(rename = "Status")]
        status: String,
        #[tabled(rename = "Action")]
        action: String,
    }

    let rows: Vec<Row> = plan
        .actions
        .iter()
        .map(|e| Row {
            root: e.path.display().to_string(),
            status: status_str(e).to_owned(),
            action: action_str(e).to_owned(),
        })
        .collect();

    let table = Table::new(rows).with(Style::modern()).to_string();
    println!("{table}");
    println!("{}", plan.summary);
}

fn main() -> ExitCode {
    sigpipe::reset();
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("anchor: error: {e}");
            ExitCode::FAILURE
        }
    }
}
