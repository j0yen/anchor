//! anchor — CLI entry point.
//!
//! Run `anchor plan` to compare the declared roots manifest against the live
//! watchman state and print a reconcile plan.
//!
//! Run `anchor reconcile` (print-only) or `anchor reconcile --apply` to
//! execute the plan and re-assert missing/stale roots.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use anchor::{
    ApplyReport, Error, FakeBackend, RootsConfig, WatchBackend, WatchState, WatchmanBackend,
    apply, reconcile,
};

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
    /// Show the reconcile plan; with --apply, execute it.
    ///
    /// Default (no --apply): print-only. Exits non-zero if any root is Missing.
    /// With --apply: executes `Watch` and `ReseedCursor` actions; never removes roots.
    ///   Safe to run repeatedly — idempotent on the second pass.
    Reconcile(ReconcileArgs),
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

#[derive(Debug, clap::Args)]
struct ReconcileArgs {
    /// Path to the roots manifest (default: ~/.config/anchor/roots.toml).
    #[arg(long)]
    manifest: Option<PathBuf>,

    /// Output format.
    #[arg(long, default_value = "table")]
    format: OutputFormat,

    /// Execute the plan: re-assert Missing roots and reseed Stale cursors.
    ///
    /// Without this flag, reconcile is print-only and makes no backend calls.
    /// Never removes undeclared roots regardless of this flag.
    #[arg(long)]
    apply: bool,

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

        Cmd::Reconcile(args) => {
            let manifest_path = args.manifest.unwrap_or_else(default_manifest_path);
            let cfg = RootsConfig::load(&manifest_path)?;

            // Select backend: fake (CI) or real watchman.
            // Box<dyn WatchBackend> lets us hold either without generics in run().
            let backend: Box<dyn WatchBackend> = if args.fake_backend {
                Box::new(FakeBackend { live: vec![] })
            } else {
                Box::new(WatchmanBackend)
            };

            let live = backend.live_roots()?;

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(duration_secs_to_i64)
                .unwrap_or(0);

            let plan = reconcile(&cfg.roots, &live, now);

            if !args.apply {
                // Print-only: no backend mutations.
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
                return Ok(ExitCode::SUCCESS);
            }

            // --apply: execute the plan.
            let report = apply(&plan, backend.as_ref());

            match args.format {
                OutputFormat::Json => {
                    let json = serde_json::to_string_pretty(&report)
                        .map_err(|e| Error::Watchman(format!("json serialize: {e}")))?;
                    println!("{json}");
                }
                OutputFormat::Table => {
                    print_apply_report(&report);
                }
            }

            if !report.is_clean() {
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

const fn apply_action_str(action: &anchor::ReconcileAction) -> &'static str {
    match action {
        anchor::ReconcileAction::Watch { .. } => "watch",
        anchor::ReconcileAction::ReseedCursor { .. } => "reseed-cursor",
        anchor::ReconcileAction::NoOp { .. } => "no-op",
        anchor::ReconcileAction::NoteUndeclared { .. } => "note-undeclared",
    }
}

fn apply_action_path(action: &anchor::ReconcileAction) -> String {
    match action {
        anchor::ReconcileAction::Watch { path }
        | anchor::ReconcileAction::ReseedCursor { path }
        | anchor::ReconcileAction::NoOp { path }
        | anchor::ReconcileAction::NoteUndeclared { path } => path.display().to_string(),
    }
}

fn print_apply_report(report: &ApplyReport) {
    use tabled::{Table, Tabled, settings::Style};

    #[derive(Tabled)]
    struct Row {
        #[tabled(rename = "Root")]
        root: String,
        #[tabled(rename = "Action")]
        action: String,
        #[tabled(rename = "Result")]
        result: String,
    }

    let mut rows: Vec<Row> = report
        .succeeded
        .iter()
        .map(|a| Row {
            root: apply_action_path(a),
            action: apply_action_str(a).to_owned(),
            result: "ok".to_owned(),
        })
        .collect();

    for (a, err) in &report.failed {
        rows.push(Row {
            root: apply_action_path(a),
            action: apply_action_str(a).to_owned(),
            result: format!("FAILED: {err}"),
        });
    }

    if rows.is_empty() {
        println!("reconcile --apply: nothing to do (all roots healthy)");
        return;
    }

    let table = Table::new(rows).with(Style::modern()).to_string();
    println!("{table}");
    println!(
        "applied {}/{} actions",
        report.succeeded.len(),
        report.attempted.len()
    );
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
