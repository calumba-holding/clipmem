#!/bin/sh
# clipmem skill — setup health check
#
# Verifies that clipmem is installed, its database is healthy, the watcher
# daemon has recent captures, and (optionally) the OpenClaw integration is
# wired up.
#
# Exit codes:
#   0  all checks passed (green) or watcher freshness is the only soft warning
#   1  watcher stale: clipmem is installed and healthy but nothing captured
#      recently and no LaunchAgent is running
#   2  binary missing: clipmem is not on PATH
#   3  doctor failed: clipmem doctor reported a hard error
#
# Intended for agents; stdout is human-readable, stderr carries diagnostics.

set -u

red()    { printf '\033[31m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }
green()  { printf '\033[32m%s\033[0m\n' "$*"; }

fail() {
    red "FAIL: $1"
    exit "$2"
}

# ---------------------------------------------------------------------------
# 1. clipmem binary
# ---------------------------------------------------------------------------
if ! command -v clipmem >/dev/null 2>&1; then
    fail "clipmem is not on PATH. Install via 'brew install tristanmanchester/tap/clipmem' or 'cargo install clipmem'." 2
fi

VERSION=$(clipmem --version 2>/dev/null || true)
green "OK: ${VERSION:-clipmem present}"

# ---------------------------------------------------------------------------
# 2. clipmem doctor
# ---------------------------------------------------------------------------
DOCTOR_OUT=$(clipmem doctor --json 2>&1)
DOCTOR_STATUS=$?

if [ "$DOCTOR_STATUS" -ne 0 ]; then
    red "FAIL: clipmem doctor exited ${DOCTOR_STATUS}"
    printf '%s\n' "$DOCTOR_OUT" >&2
    exit 3
fi

green "OK: clipmem doctor exited cleanly"

if printf '%s' "$DOCTOR_OUT" | grep -Eq '"fts5_create_virtual_table_ok"[[:space:]]*:[[:space:]]*true'; then
    green "OK: FTS5 available"
else
    yellow "WARN: FTS5 not available; --mode fts will fail. Use --mode literal."
fi

# ---------------------------------------------------------------------------
# 3. Watcher freshness: anything captured in the last hour?
# ---------------------------------------------------------------------------
TIMELINE_OUT=$(clipmem timeline --hours 1 --format json --limit 1 2>&1 || true)
if printf '%s' "$TIMELINE_OUT" | grep -q '"snapshot_id"'; then
    green "OK: clipboard capture observed in the last hour"
    FRESH=1
else
    yellow "WARN: no clipboard captures in the last hour"
    FRESH=0
fi

# ---------------------------------------------------------------------------
# 4. LaunchAgent (macOS only)
# ---------------------------------------------------------------------------
LAUNCHAGENT_RUNNING=0
if [ "$(uname -s)" = "Darwin" ]; then
    LAUNCHCTL_ROW=$(launchctl list 2>/dev/null | awk '$3 == "io.openclaw.clipmem.watch" { print; exit }')
    if [ -n "$LAUNCHCTL_ROW" ]; then
        LAUNCHAGENT_PID=$(printf '%s\n' "$LAUNCHCTL_ROW" | awk '{ print $1 }')
        if [ "$LAUNCHAGENT_PID" = "-" ]; then
            yellow "WARN: LaunchAgent io.openclaw.clipmem.watch is loaded but not running"
            yellow "      Install with: ./scripts/install_launchagent.sh (from the clipmem repo)"
        else
            green "OK: LaunchAgent io.openclaw.clipmem.watch is running (PID $LAUNCHAGENT_PID)"
            LAUNCHAGENT_RUNNING=1
        fi
    else
        yellow "WARN: LaunchAgent io.openclaw.clipmem.watch is not loaded"
        yellow "      Install with: ./scripts/install_launchagent.sh (from the clipmem repo)"
    fi
fi

# ---------------------------------------------------------------------------
# 5. OpenClaw integration (optional, best-effort)
# ---------------------------------------------------------------------------
if clipmem agents openclaw --help >/dev/null 2>&1; then
    if clipmem agents openclaw doctor >/dev/null 2>&1; then
        green "OK: clipmem agents openclaw doctor passed"
    else
        yellow "WARN: clipmem agents openclaw doctor reported issues; run it directly for details"
    fi
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
if [ "$FRESH" -eq 0 ] && [ "$LAUNCHAGENT_RUNNING" -eq 0 ]; then
    # Watcher is almost certainly stale.
    red "STALE: no recent captures and the LaunchAgent is not running. Start the watcher and retry."
    exit 1
fi

green "All checks passed."
exit 0
