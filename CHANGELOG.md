# Changelog

## v0.4.0 — 2026-06-07

Add `anchor probe` subcommand (PRD-anchor-probe). Reports watchman socket
liveness and per-root health as structured JSON (`ProbeReport`) or a human
table. Exit codes encode severity: 0=ok, 1=stale, 2=missing, 3=socket-down.
Adds `Severity`, `RootHealth`, `ProbeReport` public types (serde); `ping()`
method on `WatchBackend` with default `watchman version` impl; `FakeBackend`
gains `socket_alive` field and `::new()`/`::dead()` constructors. 11
integration tests in `tests/probe.rs` cover ACs 1–8.

## v0.3.0 — 2026-06-06

Add boot/ lifecycle hooks: anchor-reconcile.service (Type=oneshot boot unit), anchor-session-start.sh (SessionStart hook, exits 0 always), install.sh (idempotent; prints settings.json entry, does not write it). Fixes the watchman-drops-roots-on-reboot gap noted in the 2026-06-03 journal. ACs 1-4 verified offline; ACs 5-6 deferred to real reboot/session.

## v0.2.0 — 2026-06-06

Add `anchor reconcile --apply`: executes the reconcile plan (Watch/ReseedCursor
actions) against the WatchBackend. Print-only default; `--apply` is idempotent —
safe for boot oneshots and SessionStart hooks. Adds `ApplyReport` (serde),
`TrackingFakeBackend` for call-count assertions, and 6 integration tests in
`tests/reconcile.rs` covering ACs 1–5, 7–8. AC6 (live watchman) deferred.

## [Unreleased]

### Added

- Initial scaffold: `anchor plan` subcommand with table and JSON output.
- Core types: `WatchRoot`, `WatchState`, `RootStatus`, `ReconcileAction`, `ReconcilePlan`.
- `WatchBackend` trait with `WatchmanBackend` and `FakeBackend` implementations.
- Pure `reconcile()` function with injected clock for testability.
- `RootsConfig::load()` for parsing the TOML manifest.
- `config/roots.example.toml` covering the Joe Yen daily watch roots.
- 8 acceptance tests covering all MUST ACs from PRD-anchor-roots.
- SIGPIPE reset in `main()` so `anchor plan | head` never panics.
