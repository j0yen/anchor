# Changelog

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
