# Changelog

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
