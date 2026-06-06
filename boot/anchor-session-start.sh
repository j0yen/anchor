#!/usr/bin/env bash
# anchor-session-start.sh — SessionStart hook: re-assert watchman watch roots.
#
# Wired via ~/.claude/settings.json under hooks.SessionStart (user adds this
# entry after running install.sh; install.sh prints the exact snippet to add).
#
# Low-noise posture: only prints if action was taken (a root was re-asserted)
# or if anchor is missing entirely. Healthy sessions stay silent.
#
# SAFETY: exits 0 always — a watch failure must NEVER block a session start.
set -uo pipefail

# Locate the anchor binary (installed by `cargo install --path .` or copied).
ANCHOR="${ANCHOR_BIN:-${HOME}/.local/bin/anchor}"

if [[ ! -x "$ANCHOR" ]]; then
    # anchor not installed; skip silently (not an error — hook is optional).
    exit 0
fi

# Run reconcile --apply; capture output to suppress unless something changed.
OUTPUT="$("$ANCHOR" reconcile --apply 2>&1)" || true

# Print only if output is non-empty (i.e. something was re-asserted or failed).
if [[ -n "$OUTPUT" ]]; then
    printf '[anchor-session-start] %s\n' "$OUTPUT"
fi

# Always exit 0 — watch failure must not block session start.
exit 0
