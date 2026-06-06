//! anchor — declared watch-root manifest and pure reconcile plan.
//!
//! # Overview
//!
//! `anchor` maintains a versioned TOML manifest of watchman roots that *should*
//! be watched, then diffs that declared set against the live watchman state.
//! The core [`reconcile`] function is **pure**: it takes two slices + an
//! injected clock and returns a [`ReconcilePlan`] with no backend calls, no
//! filesystem side-effects.
//!
//! # Quick example
//!
//! ```no_run
//! use anchor::{RootsConfig, WatchBackend, WatchmanBackend, reconcile};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let cfg = RootsConfig::load(std::path::Path::new("~/.config/anchor/roots.toml"))?;
//! let backend = WatchmanBackend;
//! let live = backend.live_roots()?;
//! let now = i64::try_from(
//!     std::time::SystemTime::now()
//!         .duration_since(std::time::UNIX_EPOCH)?
//!         .as_secs()
//! ).unwrap_or(i64::MAX);
//! let plan = reconcile(&cfg.roots, &live, now);
//! eprintln!("{plan:#?}");
//! # Ok(())
//! # }
//! ```
//!
//! # Manifest format
//!
//! ```toml
//! [[root]]
//! path = "/home/jsy/.claude"
//! max_age_secs = 86400   # optional; omit to disable staleness check
//!
//! [[root]]
//! path = "/home/jsy/brain"
//! ```
//!
//! # `WatchBackend` trait
//!
//! Implementors must provide:
//! - [`WatchBackend::live_roots`] — query the current watch list.
//! - [`WatchBackend::watch`] — assert a new watch (used by apply-layer PRDs).
//! - [`WatchBackend::reseed_cursor`] — reset a stale wchg cursor (apply-layer).
//!
//! [`WatchmanBackend`] is the real implementation that shells `watchman`.
//! [`FakeBackend`] is provided for testing without a live watchman socket.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ─── Error ────────────────────────────────────────────────────────────────────

/// Errors returned by anchor operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// IO error reading the manifest file.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// TOML parse error in the manifest.
    #[error("toml parse: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// Watchman command failed.
    #[error("watchman: {0}")]
    Watchman(String),

    /// JSON parse error from watchman output.
    #[error("watchman json: {0}")]
    WatchmanJson(#[from] serde_json::Error),
}

/// Alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

// ─── Manifest types ───────────────────────────────────────────────────────────

/// One entry in the declared-roots manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchRoot {
    /// Absolute path to the root that should be watched.
    pub path: PathBuf,

    /// If set, a watchman clock older than this many seconds is "stale".
    /// `None` means staleness is not checked for this root.
    pub max_age_secs: Option<u64>,
}

/// The parsed `roots.toml` manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootsConfig {
    /// The declared roots, in manifest order.
    #[serde(rename = "root")]
    pub roots: Vec<WatchRoot>,
}

impl RootsConfig {
    /// Load and parse a `roots.toml` manifest from `path`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be read, or [`Error::TomlParse`]
    /// if the TOML is malformed.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&text)?;
        Ok(cfg)
    }
}

// ─── Live-state types ─────────────────────────────────────────────────────────

/// The live state of one watchman root as seen through a [`WatchBackend`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchState {
    /// Path reported by watchman.
    pub path: PathBuf,

    /// Watchman clock string (e.g. `c:1780493700:…`), if available.
    pub clock: Option<String>,

    /// Whether watchman confirms the root is currently being watched.
    pub present: bool,
}

// ─── Reconcile result types ───────────────────────────────────────────────────

/// Classification of a single declared root after comparing against live state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RootStatus {
    /// Root is watched and the clock is fresh (or no `max_age_secs` declared).
    Watched,
    /// Root is declared but not in the live watchman set.
    Missing,
    /// Root is watched but the clock is older than `max_age_secs`.
    Stale {
        /// Age of the watchman clock in seconds (from injected `now`).
        age_secs: u64,
    },
    /// Root is live in watchman but not in the manifest — informational only,
    /// never auto-removed.
    Undeclared,
}

/// The action the reconcile layer recommends for one root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ReconcileAction {
    /// Re-assert the root with `watchman watch`.
    Watch {
        /// Path to watch.
        path: PathBuf,
    },
    /// Reset the wchg cursor with `watchman since`.
    ReseedCursor {
        /// Path whose cursor should be reset.
        path: PathBuf,
    },
    /// Root is healthy; nothing to do.
    NoOp {
        /// Path that is healthy.
        path: PathBuf,
    },
    /// Record that a live root is not in the manifest (never triggers removal).
    NoteUndeclared {
        /// The undeclared live path.
        path: PathBuf,
    },
}

/// One row in the reconcile plan table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileEntry {
    /// The root path (declared or undeclared live).
    pub path: PathBuf,
    /// Computed status.
    pub status: RootStatus,
    /// Recommended action.
    pub action: ReconcileAction,
}

/// The complete output of [`reconcile`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcilePlan {
    /// Per-root entries, declared roots first then undeclared live roots.
    pub actions: Vec<ReconcileEntry>,
    /// Human-readable one-line summary.
    pub summary: String,
    /// Count of roots classified as [`RootStatus::Missing`].
    pub missing_count: usize,
}

// ─── WatchBackend trait ───────────────────────────────────────────────────────

/// Abstraction over the watchman socket.
///
/// Separates the pure [`reconcile`] logic from the watchman I/O so tests can
/// inject a [`FakeBackend`] without a live socket.
///
/// The `watch` and `reseed_cursor` methods are defined here so `anchor-reconcile`
/// (the apply PRD) can reuse the trait; `anchor plan` never calls them.
pub trait WatchBackend {
    /// Return the list of roots currently tracked by the backend.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the backend query fails.
    fn live_roots(&self) -> Result<Vec<WatchState>>;

    /// Assert a new watch on `path`.
    ///
    /// Used by the apply layer; not called by `anchor plan`.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the watch command fails.
    fn watch(&self, path: &Path) -> Result<()>;

    /// Reset the wchg cursor for `path`.
    ///
    /// Used by the apply layer; not called by `anchor plan`.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if the reseed command fails.
    fn reseed_cursor(&self, path: &Path) -> Result<()>;
}

// ─── WatchmanBackend ──────────────────────────────────────────────────────────

/// Real [`WatchBackend`] that shells out to the `watchman` CLI.
pub struct WatchmanBackend;

/// Minimal struct to deserialize `watchman watch-list` JSON output.
#[derive(Debug, Deserialize)]
struct WatchListOutput {
    roots: Vec<String>,
}

impl WatchBackend for WatchmanBackend {
    fn live_roots(&self) -> Result<Vec<WatchState>> {
        let out = std::process::Command::new("watchman")
            .args(["watch-list"])
            .output()
            .map_err(|e| Error::Watchman(format!("exec watchman: {e}")))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(Error::Watchman(format!("watch-list failed: {stderr}")));
        }

        let parsed: WatchListOutput = serde_json::from_slice(&out.stdout)?;
        let states = parsed
            .roots
            .into_iter()
            .map(|p| WatchState {
                path: PathBuf::from(p),
                clock: None, // full clock query is a future probe PRD
                present: true,
            })
            .collect();
        Ok(states)
    }

    fn watch(&self, path: &Path) -> Result<()> {
        let out = std::process::Command::new("watchman")
            .args(["watch", &path.to_string_lossy()])
            .output()
            .map_err(|e| Error::Watchman(format!("exec watchman watch: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(Error::Watchman(format!("watch failed: {stderr}")));
        }
        Ok(())
    }

    fn reseed_cursor(&self, path: &Path) -> Result<()> {
        let out = std::process::Command::new("watchman")
            .args(["watch", &path.to_string_lossy()])
            .output()
            .map_err(|e| Error::Watchman(format!("exec watchman reseed: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(Error::Watchman(format!("reseed failed: {stderr}")));
        }
        Ok(())
    }
}

// ─── FakeBackend ──────────────────────────────────────────────────────────────

/// Test-only [`WatchBackend`] that returns a pre-canned snapshot with no
/// backend calls. Verifiable: no stdout/stderr, no subprocess, purely in-memory.
pub struct FakeBackend {
    /// The snapshot of live roots returned by [`WatchBackend::live_roots`].
    pub live: Vec<WatchState>,
}

impl WatchBackend for FakeBackend {
    fn live_roots(&self) -> Result<Vec<WatchState>> {
        Ok(self.live.clone())
    }

    fn watch(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    fn reseed_cursor(&self, _path: &Path) -> Result<()> {
        Ok(())
    }
}

// ─── Pure reconcile ───────────────────────────────────────────────────────────

/// Extract the Unix timestamp embedded in a watchman clock string.
///
/// Watchman clocks look like `c:<unix-secs>:<pid>:<…>`. Returns `None` if the
/// format is unrecognised.
#[must_use]
fn clock_to_unix(clock: &str) -> Option<u64> {
    // Format: "c:<unix_secs>:<rest>"
    let mut parts = clock.split(':');
    parts.next()?; // skip "c"
    let secs_str = parts.next()?;
    secs_str.parse().ok()
}

/// Classify a present watched root's staleness status.
fn classify_staleness(root: &WatchRoot, state: &WatchState, now: i64) -> RootStatus {
    let Some(max_age) = root.max_age_secs else {
        return RootStatus::Watched;
    };
    let Some(clock) = state.clock.as_deref() else {
        return RootStatus::Watched;
    };
    clock_to_unix(clock).map_or(RootStatus::Watched, |clock_secs| {
        let now_u64 = u64::try_from(now).unwrap_or(0);
        let age = now_u64.saturating_sub(clock_secs);
        if age > max_age {
            RootStatus::Stale { age_secs: age }
        } else {
            RootStatus::Watched
        }
    })
}

/// Classify one declared root given the live index.
fn classify_root(
    root: &WatchRoot,
    live_index: &std::collections::HashMap<&Path, &WatchState>,
    now: i64,
    missing_count: &mut usize,
) -> ReconcileEntry {
    if let Some(state) = live_index.get(root.path.as_path()) {
        if state.present {
            let status = classify_staleness(root, state, now);
            let action = match &status {
                RootStatus::Stale { .. } => ReconcileAction::ReseedCursor {
                    path: root.path.clone(),
                },
                _ => ReconcileAction::NoOp {
                    path: root.path.clone(),
                },
            };
            ReconcileEntry { path: root.path.clone(), status, action }
        } else {
            *missing_count += 1;
            ReconcileEntry {
                path: root.path.clone(),
                status: RootStatus::Missing,
                action: ReconcileAction::Watch { path: root.path.clone() },
            }
        }
    } else {
        *missing_count += 1;
        ReconcileEntry {
            path: root.path.clone(),
            status: RootStatus::Missing,
            action: ReconcileAction::Watch { path: root.path.clone() },
        }
    }
}

/// Pure reconcile function — compares the declared manifest against a live
/// snapshot and emits a [`ReconcilePlan`].
///
/// # Guarantees
///
/// - Makes **zero** backend calls. It operates only on the two passed slices
///   and the injected `now` timestamp.
/// - Never removes or modifies any state; all actions are declarative.
/// - `now` is in Unix seconds; pass a real `SystemTime` or a fixed value in
///   tests.
///
/// # Undeclared roots
///
/// Live roots not in `declared` are appended to [`ReconcilePlan::actions`] as
/// [`RootStatus::Undeclared`] / [`ReconcileAction::NoteUndeclared`]. They are
/// informational only.
#[must_use]
pub fn reconcile(declared: &[WatchRoot], live: &[WatchState], now: i64) -> ReconcilePlan {
    let mut actions: Vec<ReconcileEntry> = Vec::new();

    // Index live roots by path for O(n) lookup.
    let live_index: std::collections::HashMap<&Path, &WatchState> =
        live.iter().map(|s| (s.path.as_path(), s)).collect();

    let mut missing_count = 0usize;

    // Classify each declared root.
    for root in declared {
        let entry = classify_root(root, &live_index, now, &mut missing_count);
        actions.push(entry);
    }

    // Append undeclared live roots (informational).
    let declared_paths: std::collections::HashSet<&Path> =
        declared.iter().map(|r| r.path.as_path()).collect();

    for state in live {
        if !declared_paths.contains(state.path.as_path()) {
            actions.push(ReconcileEntry {
                path: state.path.clone(),
                status: RootStatus::Undeclared,
                action: ReconcileAction::NoteUndeclared {
                    path: state.path.clone(),
                },
            });
        }
    }

    let summary = format!(
        "{} declared ({} missing, {} ok/stale), {} undeclared live",
        declared.len(),
        missing_count,
        declared.len().saturating_sub(missing_count),
        actions.len().saturating_sub(declared.len()),
    );

    ReconcilePlan {
        actions,
        summary,
        missing_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_root(path: &str) -> WatchRoot {
        WatchRoot {
            path: PathBuf::from(path),
            max_age_secs: None,
        }
    }

    fn fake_state(path: &str, present: bool) -> WatchState {
        WatchState {
            path: PathBuf::from(path),
            clock: None,
            present,
        }
    }

    #[test]
    fn reconcile_missing_root_yields_watch_action() {
        let declared = vec![fake_root("/home/jsy/.claude")];
        let live: Vec<WatchState> = vec![];
        let plan = reconcile(&declared, &live, 0);
        assert_eq!(plan.missing_count, 1);
        assert!(matches!(
            plan.actions[0].action,
            ReconcileAction::Watch { .. }
        ));
    }

    #[test]
    fn reconcile_present_root_yields_noop() {
        let declared = vec![fake_root("/home/jsy/brain")];
        let live = vec![fake_state("/home/jsy/brain", true)];
        let plan = reconcile(&declared, &live, 0);
        assert_eq!(plan.missing_count, 0);
        assert!(matches!(
            plan.actions[0].action,
            ReconcileAction::NoOp { .. }
        ));
    }

    #[test]
    fn reconcile_stale_root_yields_reseed() {
        let declared = vec![WatchRoot {
            path: PathBuf::from("/home/jsy/wintermute"),
            max_age_secs: Some(3600),
        }];
        // Clock says 1000 secs ago; now = 100_000; age = 99_000 > 3600 → Stale
        let live = vec![WatchState {
            path: PathBuf::from("/home/jsy/wintermute"),
            clock: Some("c:900:1234:0".to_owned()),
            present: true,
        }];
        let plan = reconcile(&declared, &live, 100_000);
        assert!(matches!(plan.actions[0].status, RootStatus::Stale { .. }));
        assert!(matches!(
            plan.actions[0].action,
            ReconcileAction::ReseedCursor { .. }
        ));
    }

    #[test]
    fn reconcile_undeclared_live_root_noted() {
        let declared: Vec<WatchRoot> = vec![];
        let live = vec![fake_state("/tmp/random", true)];
        let plan = reconcile(&declared, &live, 0);
        assert_eq!(plan.actions.len(), 1);
        assert!(matches!(plan.actions[0].status, RootStatus::Undeclared));
    }

    #[test]
    fn clock_to_unix_parses_correctly() {
        assert_eq!(clock_to_unix("c:1780493700:1234:0"), Some(1_780_493_700));
        assert_eq!(clock_to_unix("not-a-clock"), None);
        assert_eq!(clock_to_unix("c:notanumber:0"), None);
    }
}
