# anchor

A declared manifest of which watchman roots should be live, and a pure function that diffs declared against live to produce a reconcile plan.

## Why it exists

A watchman-backed laptop loses state quietly. Every reboot — or socket bounce — drops the watched roots, and `wchg` then returns empty deltas with no error to tell you why. Nothing watches the watcher: there is no canonical record of which roots *should* be live, and nothing that compares that record to reality.

`anchor` is that record. You declare the roots in a TOML file; `anchor` reads the live watchman set and reports the difference. The diff is computed by a pure function — no backend calls, deterministic given its inputs — so the part that decides what to do is fully testable, and the part that touches watchman is a thin, swappable layer around it.

This crate is the base of a small fleet. `anchor-probe` adds cursor-age probing, `anchor-reconcile` adds the `--apply` path that re-asserts dropped roots, and `anchor-boot` wires reconcile into the laptop's lifecycle. All three extend the types and the `WatchBackend` trait defined here.

## Install

```
cargo install --path .
```

MSRV is Rust 1.85 (edition 2024, no `let-chains`).

## Quick start

```
# Create your manifest
mkdir -p ~/.config/anchor
cp config/roots.example.toml ~/.config/anchor/roots.toml
$EDITOR ~/.config/anchor/roots.toml

# Show the reconcile plan
anchor plan
anchor plan --format json
```

`anchor plan` exits non-zero if any declared root is `Missing`, so it works as a pre-command gate. An example manifest covering the daily roots ships at `config/roots.example.toml`.

## Manifest format (`roots.toml`)

```toml
# Each [[root]] declares one watchman root that should always be live.

[[root]]
path = "/home/jsy/.claude"
max_age_secs = 86400      # optional: clock older than this → "stale"

[[root]]
path = "/home/jsy/brain"
# no max_age_secs: staleness not checked
```

The file is loaded by `RootsConfig::load(path)` into `Vec<WatchRoot>`.

## Commands

| Command | What it does |
|---|---|
| `anchor plan` | diff declared roots against live watchman; print the reconcile plan |
| `anchor probe` | report socket liveness and per-root health with a severity exit code |
| `anchor reconcile` | the reconcile plan plus an `--apply` path that re-asserts dropped roots |

Both `plan` and `probe` take `--format json` for machine-readable output.

### `anchor probe` exit codes

The probe reports health through its exit code so a hook can branch on watch health without parsing human output. Highest severity wins.

| Code | Meaning |
|------|---------|
| `0` | all roots watched and fresh; socket alive |
| `1` | at least one root has a stale clock |
| `2` | at least one root is missing from the live set |
| `3` | watchman socket unreachable (root checks skipped) |

JSON output is a `ProbeReport`; the top-level `worst` field is one of `"ok"`, `"stale"`, `"missing"`, `"socket_down"`.

## How it works

The core is one function:

```rust
pub fn reconcile(declared: &[WatchRoot], live: &[WatchState], now: i64) -> ReconcilePlan
```

It makes zero backend calls, is deterministic given its inputs, and never removes or modifies state — every output is declarative. Live roots not in the manifest are appended as `Undeclared` and noted, never auto-removed.

The watchman socket sits behind a trait, so the diff logic never touches a subprocess and a backend swap never touches the diff:

```rust
pub trait WatchBackend {
    fn live_roots(&self) -> Result<Vec<WatchState>>;   // query current watch list
    fn watch(&self, path: &Path) -> Result<()>;        // re-assert watch (apply layer)
    fn reseed_cursor(&self, path: &Path) -> Result<()>;// reset cursor (apply layer)
    fn ping(&self) -> Result<bool>;                    // socket liveness (probe layer)
}
```

`WatchmanBackend` shells out to `watchman`; `FakeBackend` is in-memory for tests. The downstream crates extend the fleet by implementing more of this trait or wrapping it.

### Core types

All are public and `serde`-(de)serializable — they are the extension surface for the rest of the fleet.

- `WatchRoot` — one declared entry: a `path` and optional `max_age_secs`.
- `WatchState` — one live root as seen through a backend: `path`, optional watchman `clock`, `present`.
- `RootStatus` — `Watched` / `Missing` / `Stale { age_secs }` / `Undeclared`.
- `ReconcileAction` — `Watch` / `ReseedCursor` / `NoOp` / `NoteUndeclared`, each carrying a `path`.
- `ReconcileEntry` — pairs a `path`, `status`, and `action`.
- `ReconcilePlan` — the `actions`, a one-line `summary`, and `missing_count` (which drives the exit code).

## anchor-boot: lifecycle wiring

`anchor-boot` ships the `boot/` directory, which wires `anchor reconcile --apply` into the laptop's lifecycle so roots are restored without a manual re-watch. Two loss windows, two mechanisms:

| Loss window | Mechanism |
|---|---|
| reboot / watchman restart | `anchor-reconcile.service` — `Type=oneshot`, ordered `After=watchman.service` |
| new Claude session (mid-session loss) | `anchor-session-start.sh` SessionStart hook |

```bash
bash boot/install.sh
```

`install.sh` is idempotent — run it twice and you get exactly one unit symlink and exit 0. It symlinks the service into `~/.config/systemd/user/`, runs `daemon-reload && enable`, and prints the SessionStart hook line to add. It does not edit `~/.claude/settings.json`; that edit is user-gated.

To back out:

```bash
systemctl --user disable --now anchor-reconcile.service
rm ~/.config/systemd/user/anchor-reconcile.service
# then remove the SessionStart hook line from ~/.claude/settings.json by hand
```

`boot/anchor-reconcile.timer` ships but is not enabled by default — the oneshot plus the SessionStart hook already cover both observed loss windows. Enable it with `systemctl --user enable --now anchor-reconcile.timer` if you want a periodic check.

The session-start hook stays quiet: it prints one line only if a root was re-asserted, and always exits 0, so a watch failure never blocks a session from starting.

### SessionStart hook usage

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "anchor probe --format json > /tmp/anchor-probe.json 2>&1; code=$?; if [ $code -ge 2 ]; then echo 'anchor: watch health degraded (exit '$code') — run anchor reconcile --apply'; fi; exit 0"
          }
        ]
      }
    ]
  }
}
```

Passes silently on `0`/`1`, warns on `2`/`3`, always exits 0.

## SIGPIPE

`main()` calls `sigpipe::reset()` first thing, so `anchor plan | head` and `anchor probe | head -1` never panic.

## License

MIT OR Apache-2.0
