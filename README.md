# anchor

**Declared watch-root manifest and pure reconcile plan for watchman root management.**

`anchor` solves a recurring problem on a watchman-backed laptop: every reboot
(or socket bounce) silently drops all watched roots, leaving `wchg` returning
empty deltas with no diagnostic. There is no canonical record of which roots
*should* be watched, and no tool that diffs declared-vs-live.

`anchor` is the base crate that creates that record and the shared types. The
rest of the anchor fleet extends it:

| PRD | What it adds |
|---|---|
| `anchor-probe` | real-time cursor-age probing via watchman clock |
| `anchor-reconcile` | `--apply` mode that calls `watchman watch` / reseeds cursors |
| `anchor-boot` | systemd service that re-watches roots on every reboot |

---

## Quick start

```
# Install
cargo install --path .

# Create your manifest
mkdir -p ~/.config/anchor
cp config/roots.example.toml ~/.config/anchor/roots.toml
$EDITOR ~/.config/anchor/roots.toml

# Show the reconcile plan
anchor plan
anchor plan --format json
```

`anchor plan` exits **non-zero** if any declared root is `Missing` — safe to
use as a pre-command gate.

---

## Manifest format (`roots.toml`)

```toml
# Each [[root]] entry declares one watchman root that should always be live.

[[root]]
path = "/home/jsy/.claude"
max_age_secs = 86400      # optional: clock older than this → "stale"

[[root]]
path = "/home/jsy/brain"
# no max_age_secs: staleness not checked
```

The file is loaded by `RootsConfig::load(path)` and parsed into `Vec<WatchRoot>`.
An example covering the Joe Yen daily roots ships at `config/roots.example.toml`.

---

## Core types

All types are public and `serde`-(de)serializable. They form the extension
surface for `anchor-probe`, `anchor-reconcile`, and `anchor-boot`.

### `WatchRoot`

One entry in the declared-roots manifest:

```rust
pub struct WatchRoot {
    pub path: PathBuf,
    pub max_age_secs: Option<u64>,
}
```

### `WatchState`

One live watchman root as seen through a `WatchBackend`:

```rust
pub struct WatchState {
    pub path: PathBuf,
    pub clock: Option<String>,  // watchman clock string, e.g. "c:1780493700:…"
    pub present: bool,
}
```

### `RootStatus`

```rust
pub enum RootStatus {
    Watched,                   // live and clock is fresh (or no max_age_secs)
    Missing,                   // declared but not in live set
    Stale { age_secs: u64 },  // live but clock is older than max_age_secs
    Undeclared,                // live but not in the manifest (informational only)
}
```

### `ReconcileAction`

```rust
pub enum ReconcileAction {
    Watch { path }             // re-assert with `watchman watch`
    ReseedCursor { path }      // reset wchg cursor
    NoOp { path }              // root is healthy
    NoteUndeclared { path }    // record undeclared live root (never auto-removed)
}
```

### `ReconcilePlan`

```rust
pub struct ReconcilePlan {
    pub actions: Vec<ReconcileEntry>,  // declared first, undeclared live appended
    pub summary: String,               // one-line human summary
    pub missing_count: usize,          // count of Missing roots (drives exit code)
}
```

`ReconcileEntry` pairs a `path`, `status`, and `action`.

---

## `WatchBackend` trait

Abstracts the watchman socket so `reconcile()` is pure and testable, and so a
backend swap (or watchman replacement) never touches the diff logic.

```rust
pub trait WatchBackend {
    fn live_roots(&self) -> Result<Vec<WatchState>>;   // query current watch list
    fn watch(&self, path: &Path) -> Result<()>;        // re-assert watch (apply layer)
    fn reseed_cursor(&self, path: &Path) -> Result<()>;// reset cursor (apply layer)
    fn ping(&self) -> Result<bool>;                    // socket liveness (probe layer)
}
```

Two implementations ship:

- **`WatchmanBackend`** — shells `watchman watch-list` / `watchman watch` / `watchman version`.
- **`FakeBackend::new(live)`** / **`FakeBackend::dead()`** — in-memory, no subprocess. Use in tests.

`anchor-probe`, `anchor-reconcile`, and `anchor-boot` extend this crate by
implementing additional `WatchBackend` methods or wrapping the existing trait.

---

## Pure `reconcile` function

```rust
pub fn reconcile(
    declared: &[WatchRoot],
    live: &[WatchState],
    now: i64,                  // Unix timestamp in seconds (injectable in tests)
) -> ReconcilePlan
```

**Guarantees:**
- Makes **zero** backend calls — accepts plain slices.
- Deterministic given the same inputs.
- Never removes or modifies state; all output is declarative.
- Appends `Undeclared` entries for live roots not in `declared`; never triggers removal.

---

## anchor-boot: lifecycle wiring

`anchor-boot` ships the `boot/` directory that wires `anchor reconcile --apply`
into the laptop's lifecycle so watch roots are restored automatically on every
reboot and every new Claude session — without any manual re-watch step.

### Coverage split

| Loss window | Mechanism |
|---|---|
| Reboot / watchman restart | `anchor-reconcile.service` — Type=oneshot, ordered After=watchman.service |
| New Claude session (mid-session loss) | `anchor-session-start.sh` SessionStart hook |

### Install

```bash
# From the anchor repo root:
bash boot/install.sh
```

`install.sh` is idempotent: running it twice leaves exactly one unit symlink and
exits 0. It:

1. Symlinks `boot/anchor-reconcile.service` into `~/.config/systemd/user/`
2. Runs `systemctl --user daemon-reload && enable`
3. **Prints** the `settings.json` SessionStart hook entry to add (it does NOT
   modify `~/.claude/settings.json` unprompted — that edit is user-gated)

### Back out

```bash
systemctl --user disable --now anchor-reconcile.service
rm ~/.config/systemd/user/anchor-reconcile.service
# Remove the SessionStart hook line from ~/.claude/settings.json by hand.
```

### Optional periodic timer

`boot/anchor-reconcile.timer` is provided but **not enabled by default**
(the oneshot + SessionStart hook already cover both observed loss windows).
Enable with:

```bash
systemctl --user enable --now anchor-reconcile.timer
```

### Session-start hook behaviour

`boot/anchor-session-start.sh` follows the low-noise posture of the existing
hooks: it prints a one-line summary **only if a root was re-asserted**. A
healthy session stays silent. It always exits 0 — a watch failure must never
block a session from starting.

---

## `anchor probe` — health check

`anchor probe` reports watchman socket liveness and per-root health with a
structured exit code so a hook can branch on watch health without parsing
human output.

```
anchor probe                   # human table (default)
anchor probe --format json     # machine-readable ProbeReport
```

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | All roots watched and fresh; socket alive |
| `1` | At least one root has a stale clock |
| `2` | At least one root is missing from the live watchman set |
| `3` | Watchman socket is unreachable (root health checks skipped) |

The highest severity wins when multiple roots have different statuses.

### JSON schema (`ProbeReport`)

```json
{
  "socket_alive": true,
  "worst": "missing",
  "checked_at": 1749344410,
  "roots": [
    {
      "path": "/home/jsy/.claude",
      "status": { "status": "missing" },
      "clock": null,
      "age_secs": null
    }
  ]
}
```

`worst` values: `"ok"`, `"stale"`, `"missing"`, `"socket_down"`.

### SessionStart hook usage

Gate Claude session startup on watch health by adding to `~/.claude/settings.json`:

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

This silently passes on `0`/`1` (ok/stale); prints a warning on `2` (missing)
or `3` (socket down); always exits 0 so a probe failure never blocks session start.

---

## SIGPIPE

`main()` calls `sigpipe::reset()` as its first statement, so `anchor plan | head` and `anchor probe | head -1` never panic.

---

## MSRV

Rust 1.85. No `let-chains`.

## License

MIT OR Apache-2.0
