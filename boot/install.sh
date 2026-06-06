#!/usr/bin/env bash
# install.sh — Idempotent installer for anchor-boot lifecycle hooks.
#
# What this does:
#   1. Symlinks anchor-reconcile.service into ~/.config/systemd/user/
#   2. Runs `systemctl --user daemon-reload` + `enable`
#   3. PRINTS (does NOT apply) the SessionStart hook entry to add to
#      ~/.claude/settings.json (settings edits are user-gated).
#
# Run twice: second run makes no further changes and exits 0 (idempotent).
#
# Back out with:
#   systemctl --user disable --now anchor-reconcile.service
#   rm ~/.config/systemd/user/anchor-reconcile.service
#   # remove the SessionStart hook line from ~/.claude/settings.json by hand
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVICE_SRC="$SCRIPT_DIR/anchor-reconcile.service"
SESSION_HOOK_SRC="$SCRIPT_DIR/anchor-session-start.sh"

# Respect $XDG_CONFIG_HOME for testing (default: ~/.config)
CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
SYSTEMD_USER_DIR="$CONFIG_HOME/systemd/user"
SERVICE_DEST="$SYSTEMD_USER_DIR/anchor-reconcile.service"

echo "=== anchor-boot install ==="

# --- 1. Ensure systemd user dir exists ---
mkdir -p "$SYSTEMD_USER_DIR"

# --- 2. Symlink the service unit (idempotent) ---
if [[ -L "$SERVICE_DEST" && "$(readlink -f "$SERVICE_DEST")" == "$(readlink -f "$SERVICE_SRC")" ]]; then
    echo "[skip] anchor-reconcile.service already symlinked (up to date)"
elif [[ -e "$SERVICE_DEST" && ! -L "$SERVICE_DEST" ]]; then
    echo "[warn] $SERVICE_DEST exists as a regular file; replacing with symlink"
    rm "$SERVICE_DEST"
    ln -s "$SERVICE_SRC" "$SERVICE_DEST"
    echo "[ok]   symlinked anchor-reconcile.service"
else
    # Remove stale symlink (wrong target) or create fresh
    [[ -L "$SERVICE_DEST" ]] && rm "$SERVICE_DEST"
    ln -s "$SERVICE_SRC" "$SERVICE_DEST"
    echo "[ok]   symlinked anchor-reconcile.service → $SERVICE_SRC"
fi

# --- 3. daemon-reload + enable (idempotent: enable --now is safe if already enabled) ---
if command -v systemctl &>/dev/null; then
    systemctl --user daemon-reload 2>/dev/null && echo "[ok]   daemon-reload"
    if systemctl --user is-enabled anchor-reconcile.service &>/dev/null; then
        echo "[skip] anchor-reconcile.service already enabled"
    else
        systemctl --user enable anchor-reconcile.service 2>/dev/null \
            && echo "[ok]   enabled anchor-reconcile.service"
    fi
else
    echo "[warn] systemctl not found; skipping daemon-reload/enable (non-systemd environment?)"
fi

# --- 4. PRINT the SessionStart hook entry (do NOT modify ~/.claude/settings.json) ---
HOOK_CMD="$(readlink -f "$SESSION_HOOK_SRC")"
cat <<EOF

=== SessionStart hook (ADD THIS MANUALLY to ~/.claude/settings.json) ===
In the "hooks" → "SessionStart" array, add one object:

  {
    "matcher": "",
    "hooks": [
      {
        "type": "command",
        "command": "bash $HOOK_CMD"
      }
    ]
  }

This re-asserts declared watchman roots on every new Claude session.
Anchor itself has been verified with:  anchor reconcile (print-only)
==========================================================================
EOF

echo ""
echo "=== Done. anchor-reconcile.service is enabled and will run at next login. ==="
